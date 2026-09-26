//! User corrections. User decisions always override automatic ones and are never undone by
//! re-processing (links and places get `origin = user` / `is_user_verified`).

use anyhow::{anyhow, Result};

use super::merge::{self, attach, AttachRequest, Evidence};
use super::Pipeline;
use crate::db::new_id;
use crate::models::*;
use crate::services::ai::types::ExtractedPlace;

impl Pipeline {
    pub async fn confirm_travel(&self, screenshot_id: &str) -> Result<ProcessingStatus> {
        self.db.set_user_classification(screenshot_id, Some(Classification::Travel))?;
        for r in self.db.reviews_for_screenshot(screenshot_id)? {
            if r.kind == ReviewKind::TravelClassification && !r.is_resolved {
                self.db.resolve_review(&r.id, "confirmed travel")?;
            }
        }
        self.resolve_from_cache(screenshot_id).await
    }

    pub fn mark_not_travel(&self, screenshot_id: &str) -> Result<()> {
        self.db.set_user_classification(screenshot_id, Some(Classification::NotTravel))?;
        self.db.clear_derived(screenshot_id, true)?;
        self.db.set_classification(screenshot_id, Classification::NotTravel, None, 0)?;
        self.db.finish_screenshot(screenshot_id, ProcessingStatus::NotTravel, Some("marked not travel by you"))?;
        Ok(())
    }

    pub fn ignore(&self, screenshot_id: &str) -> Result<()> {
        // It stays under "Ignored" but no longer affects places, the map, trips or Review.
        self.db.detach_source(Some(screenshot_id), None)?;
        self.db.finish_screenshot(screenshot_id, ProcessingStatus::Ignored, Some("ignored by you"))?;
        Ok(())
    }

    pub async fn reprocess(&self, screenshot_id: &str) -> Result<ProcessingStatus> {
        self.db.mark_for_reprocess("selected", &[screenshot_id.to_string()])?;
        Ok(self.process(screenshot_id).await)
    }

    /// Links a screenshot to a place the user picked (from the Inbox or "Add place").
    pub fn user_attach(&self, screenshot_id: &str, candidate: &PlaceCandidate, extracted: Option<&ExtractedPlace>) -> Result<String> {
        let shot = self.db.screenshot(screenshot_id)?.ok_or_else(|| anyhow!("screenshot not found"))?;
        let blocks = self.db.ocr_blocks(screenshot_id)?;
        let outcome = attach(&self.db, AttachRequest {
            candidate, extracted, evidence: Evidence::Screenshot(&shot), blocks: &blocks, line_sources: &[], confidence: 1.0,
            verification: Verification::UserVerified, origin: DataOrigin::User,
        })?;
        self.db.set_place_user_verified(&outcome.place_id)?;
        if shot.user_classification().is_none() && shot.classification != Classification::Travel {
            self.db.set_classification(screenshot_id, Classification::Travel, None, 0)?;
        }
        Ok(outcome.place_id)
    }

    /// Replaces a wrong place on a screenshot with the correct one.
    pub fn correct_place(&self, screenshot_id: &str, wrong_place_id: &str, candidate: &PlaceCandidate) -> Result<String> {
        let name = self.db.links_for_screenshot(screenshot_id)?
            .into_iter().find(|l| l.place_id == wrong_place_id).map(|l| l.extracted_name);
        self.db.unlink(wrong_place_id, screenshot_id)?;
        self.drop_if_orphan(wrong_place_id)?;
        let extracted = name.map(|n| ExtractedPlace::named(&n));
        self.user_attach(screenshot_id, candidate, extracted.as_ref())
    }

    fn drop_if_orphan(&self, place_id: &str) -> Result<()> {
        if let Some(p) = self.db.place(place_id)? {
            if p.source_count == 0 && !p.is_user_verified && p.notes.is_empty() && p.personal_status == PersonalStatus::WantToVisit {
                self.db.delete_place(place_id)?;
            }
        }
        Ok(())
    }

    /// Resolves a Review Inbox item. `action` depends on the kind:
    /// travelClassification: yes | no · placeResolution: choose | confirm | dismiss
    /// duplicatePlace: merge | separate · photoCrop: accept | reject · processingFailure: retry | ignore
    pub async fn resolve_review(&self, review_id: &str, action: &str, candidate: Option<PlaceCandidate>) -> Result<()> {
        let review = self.db.review(review_id)?.ok_or_else(|| anyhow!("review item not found"))?;
        let sid = review.screenshot_id.clone().unwrap_or_default();
        let extracted: Option<ExtractedPlace> = review.extracted_place.clone().and_then(|v| serde_json::from_value(v).ok());
        if let Some(reel_id) = review.reel_id.clone() {
            return self.resolve_reel_review(&review, &reel_id, action, candidate, extracted.as_ref()).await;
        }
        match (review.kind, action) {
            (ReviewKind::TravelClassification, "yes") => {
                self.db.resolve_review(review_id, "travel")?;
                self.confirm_travel(&sid).await?;
                return Ok(());
            }
            (ReviewKind::TravelClassification, "no") => {
                self.db.resolve_review(review_id, "not travel")?;
                self.mark_not_travel(&sid)?;
                return Ok(());
            }
            (ReviewKind::PlaceResolution, "confirm") => {
                let place_id = review.place_a_id.clone().ok_or_else(|| anyhow!("no provisional place"))?;
                self.db.set_place_user_verified(&place_id)?;
                self.db.link(&place_id, &sid, "", 1.0, DataOrigin::User)?;
                self.db.resolve_review(review_id, "confirmed")?;
            }
            (ReviewKind::PlaceResolution, "choose") => {
                let cand = candidate.ok_or_else(|| anyhow!("no candidate chosen"))?;
                if let Some(provisional) = review.place_a_id.as_deref() {
                    // Keep the facts (and their OCR provenance): move them rather than re-extracting.
                    let names_only = extracted.clone().map(|e| ExtractedPlace { facts: None, ..e });
                    let chosen = self.user_attach(&sid, &cand, names_only.as_ref())?;
                    if chosen != provisional {
                        self.db.move_source_evidence(provisional, &chosen, Some(&sid), None)?;
                        self.db.unlink(provisional, &sid)?;
                        self.drop_if_orphan(provisional)?;
                    }
                } else {
                    self.user_attach(&sid, &cand, extracted.as_ref())?;
                }
                self.db.resolve_review(review_id, "chose place")?;
            }
            (ReviewKind::PlaceResolution, "dismiss") => {
                if let Some(provisional) = review.place_a_id.as_deref() {
                    self.db.unlink(provisional, &sid)?;
                    self.drop_if_orphan(provisional)?;
                }
                self.db.resolve_review(review_id, "not a place")?;
            }
            (ReviewKind::DuplicatePlace, "merge") => {
                let (a, b) = (review.place_a_id.clone().unwrap_or_default(), review.place_b_id.clone().unwrap_or_default());
                merge::merge_places(&self.db, &a, &b)?;
                self.db.resolve_review(review_id, "merged")?;
            }
            (ReviewKind::DuplicatePlace, "separate") => self.db.resolve_review(review_id, "kept separate")?,
            (ReviewKind::PhotoCrop, "accept") => {
                if let Some(image_id) = &review.image_id {
                    self.db.set_image_accepted(image_id, true)?;
                }
                self.db.resolve_review(review_id, "accepted")?;
            }
            (ReviewKind::PhotoCrop, "reject") => {
                if let Some(image_id) = &review.image_id {
                    if let Some(img) = self.db.image(image_id)? {
                        if let Some(p) = img.image_path { let _ = std::fs::remove_file(p); }
                    }
                    self.db.delete_image(image_id)?;
                }
                self.db.resolve_review(review_id, "rejected")?;
            }
            (ReviewKind::ProcessingFailure, "retry") => {
                self.db.resolve_review(review_id, "retried")?;
                self.reprocess(&sid).await?;
                return Ok(());
            }
            (ReviewKind::ProcessingFailure, "ignore") => {
                self.db.resolve_review(review_id, "ignored")?;
                self.ignore(&sid)?;
                return Ok(());
            }
            (kind, action) => return Err(anyhow!("unsupported action {action} for {kind}")),
        }
        self.refresh_after_review(&sid)
    }

    /// When the last open review for a screenshot is handled, it becomes complete.
    fn refresh_after_review(&self, screenshot_id: &str) -> Result<()> {
        if screenshot_id.is_empty() {
            return Ok(());
        }
        let open = self.db.reviews_for_screenshot(screenshot_id)?.iter().any(|r| !r.is_resolved);
        if let Some(s) = self.db.screenshot(screenshot_id)? {
            if !open && s.status == ProcessingStatus::NeedsReview {
                self.db.finish_screenshot(screenshot_id, ProcessingStatus::Complete, None)?;
            }
        }
        Ok(())
    }

    /// Manual crop from the screenshot detail screen.
    pub async fn save_manual_crop(&self, screenshot_id: &str, place_id: &str, rect: Rect) -> Result<String> {
        let image_path = self.data_dir.join("screenshots").join(format!("{screenshot_id}.jpg"));
        if !image_path.exists() {
            let shot = self.db.screenshot(screenshot_id)?.ok_or_else(|| anyhow!("screenshot not found"))?;
            self.export_source(&shot.photos_id, &image_path).await?;
        }
        let rect = rect.clamped();
        let crop_path = self.data_dir.join("crops").join(format!("{}.jpg", new_id()));
        self.photos.crop_image(&image_path, &crop_path, rect, 1600).await?;
        let print = self.vision.feature_print(&image_path, Some(rect)).await.ok();
        let id = self.db.insert_image(place_id, Some(screenshot_id), rect, &crop_path.to_string_lossy(), 0.9, RegionType::Photograph,
                                      1.0, print.as_deref(), true, DataOrigin::User)?;
        self.db.link(place_id, screenshot_id, "", 1.0, DataOrigin::User)?;
        Ok(id)
    }

    /// "Why did I save this?" — generated only from the place's saved facts.
    pub async fn summarize_place(&self, place_id: &str) -> Result<String> {
        let place = self.db.place(place_id)?.ok_or_else(|| anyhow!("place not found"))?;
        let facts = self.db.facts_for_place(place_id)?;
        if facts.is_empty() {
            return Err(anyhow!("No saved information to summarise yet."));
        }
        let numbered: Vec<(String, String)> = facts.iter().take(40).map(|f| (f.id.clone(), f.text.clone())).collect();
        let r = self.ai().summarize_place(&place.canonical_name, &numbered).await?;
        self.db.log_ai_usage(None, "summary", &r.usage, true, crate::config::AI_PROMPT_VERSION);
        let used: Vec<String> = r.value.used_fact_ids.unwrap_or_default().into_iter()
            .filter(|id| facts.iter().any(|f| &f.id == id)).collect();
        self.db.set_place_summary(place_id, r.value.summary.trim(), &used)?;
        Ok(r.value.summary)
    }
}

impl Pipeline {
    /// Review Inbox actions for items that came from a Reel.
    async fn resolve_reel_review(&self, review: &crate::db::ReviewRecord, reel_id: &str, action: &str,
                                 candidate: Option<PlaceCandidate>, extracted: Option<&ExtractedPlace>) -> Result<()> {
        match (review.kind, action) {
            (ReviewKind::PlaceResolution, "confirm") => {
                let place_id = review.place_a_id.clone().ok_or_else(|| anyhow!("no provisional place"))?;
                self.db.set_place_user_verified(&place_id)?;
                self.db.link_reel(&place_id, reel_id, extracted.map(|e| e.display_name.as_str()).unwrap_or(""), 1.0, DataOrigin::User)?;
                self.db.resolve_review(&review.id, "confirmed")?;
            }
            (ReviewKind::PlaceResolution, "choose") => {
                let cand = candidate.ok_or_else(|| anyhow!("no candidate chosen"))?;
                if let Some(provisional) = review.place_a_id.as_deref() {
                    // Keep the facts with their Reel timestamps: move them to the chosen place.
                    let names_only = extracted.cloned().map(|e| ExtractedPlace { facts: None, ..e });
                    let chosen = self.user_attach_reel(reel_id, &cand, names_only.as_ref())?;
                    if chosen != provisional {
                        self.db.move_source_evidence(provisional, &chosen, None, Some(reel_id))?;
                        self.db.unlink_reel(provisional, reel_id)?;
                        self.drop_if_orphan(provisional)?;
                    }
                } else {
                    self.user_attach_reel(reel_id, &cand, extracted)?;
                }
                self.db.resolve_review(&review.id, "chose place")?;
            }
            (ReviewKind::PlaceResolution, "dismiss") => {
                if let Some(provisional) = review.place_a_id.as_deref() {
                    self.db.unlink_reel(provisional, reel_id)?;
                    self.drop_if_orphan(provisional)?;
                }
                self.db.resolve_review(&review.id, "not a place")?;
            }
            (ReviewKind::DuplicatePlace, "merge") => {
                let (a, b) = (review.place_a_id.clone().unwrap_or_default(), review.place_b_id.clone().unwrap_or_default());
                merge::merge_places(&self.db, &a, &b)?;
                self.db.resolve_review(&review.id, "merged")?;
            }
            (ReviewKind::DuplicatePlace, "separate") => self.db.resolve_review(&review.id, "kept separate")?,
            (ReviewKind::PhotoCrop, "accept") => {
                if let Some(image_id) = &review.image_id {
                    self.db.set_image_accepted(image_id, true)?;
                }
                self.db.resolve_review(&review.id, "accepted")?;
            }
            (ReviewKind::PhotoCrop, "reject") => {
                if let Some(image_id) = &review.image_id {
                    self.db.delete_image(image_id)?;
                }
                self.db.resolve_review(&review.id, "rejected")?;
            }
            (ReviewKind::ProcessingFailure, "retry") => {
                self.db.resolve_review(&review.id, "retried")?;
                self.reprocess_reel(reel_id).await;
                return Ok(());
            }
            (ReviewKind::ProcessingFailure, "ignore") => {
                self.db.resolve_review(&review.id, "ignored")?;
                self.db.finish_reel(reel_id, ProcessingStatus::Ignored, Some("ignored by you"))?;
                return Ok(());
            }
            (kind, action) => return Err(anyhow!("unsupported action {action} for {kind}")),
        }
        let open = self.db.reviews_for_reel(reel_id)?.iter().any(|r| !r.is_resolved);
        if let Some(reel) = self.db.reel(reel_id)? {
            if !open && reel.status == ProcessingStatus::NeedsReview {
                self.db.finish_reel(reel_id, ProcessingStatus::Complete, None)?;
            }
        }
        Ok(())
    }
}
