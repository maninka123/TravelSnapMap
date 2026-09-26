//! Place de-duplication and evidence merging. Deterministic — never uses AI.

use anyhow::Result;
use rusqlite::params;

use crate::db::{Database, FactProvenance, FactRecord, OcrBlockRecord, PlaceRecord, ReelRecord, ScreenshotRecord};
use crate::models::*;
use crate::services::ai::types::ExtractedPlace;
use crate::text::{best_similarity, distance_m, name_similarity, normalize};

#[derive(Debug, Clone)]
pub enum PlaceMatch {
    /// Same real place: merge into it.
    Same(PlaceRecord),
    /// Close and similar but not certain: keep separate, ask the user.
    Possible(PlaceRecord),
    None,
}

/// Strongest signals first: map identifier, then distance + name similarity (incl. alternative names).
pub fn find_match(db: &Database, cand: &PlaceCandidate, extra_names: &[String]) -> Result<PlaceMatch> {
    if let Some(map_id) = cand.map_identifier.as_deref() {
        if let Some(p) = db.place_by_map_identifier(map_id)? {
            return Ok(PlaceMatch::Same(p));
        }
    }
    let mut names = vec![cand.name.clone()];
    names.extend(extra_names.iter().cloned());

    let mut best_same: Option<(f64, PlaceRecord)> = None;
    let mut possible: Option<PlaceRecord> = None;
    for place in db.places_near(cand.latitude, cand.longitude, 0.003)? {
        let d = distance_m(place.latitude, place.longitude, cand.latitude, cand.longitude);
        let sim = names.iter().map(|n| best_similarity(n, &place.all_names())).fold(0.0, f64::max);
        // MapKit says these are different entities: never auto-merge them on name alone.
        let conflicting_ids = matches!((&place.map_identifier, &cand.map_identifier), (Some(a), Some(b)) if a != b);
        let same = (d < 200.0 && sim >= 0.6) || (d < 40.0 && sim >= 0.35);
        if same && !conflicting_ids {
            if best_same.as_ref().is_none_or(|(s, _)| sim > *s) {
                best_same = Some((sim, place));
            }
        } else if (same || (d < 300.0 && sim >= 0.3)) && possible.is_none() {
            possible = Some(place);
        }
    }
    Ok(match (best_same, possible) {
        (Some((_, p)), _) => PlaceMatch::Same(p),
        (None, Some(p)) => PlaceMatch::Possible(p),
        _ => PlaceMatch::None,
    })
}

/// Same fact already saved for this place? Contradicting or changed values are NOT duplicates
/// and are kept side by side (e.g. "sunrise" vs "sunset", ¥2,000 in 2024 vs ¥2,500 in 2026).
pub fn is_duplicate_fact(existing: &[FactRecord], kind: TravelFactType, text: &str) -> bool {
    let norm = normalize(text);
    // OCR of the same caption on several key frames gives near-identical text; 0.85 catches those while
    // genuinely different advice ("sunrise" vs "sunset") stays well below it.
    existing.iter().any(|f| f.fact_type == kind && (normalize(&f.text) == norm || name_similarity(&f.text, text) >= 0.85))
}

/// The evidence a place is being attached from.
#[derive(Clone, Copy)]
pub enum Evidence<'a> {
    Screenshot(&'a ScreenshotRecord),
    Reel(&'a ReelRecord),
}

/// Provenance of one numbered line sent to the AI.
#[derive(Debug, Clone, PartialEq)]
pub struct LineSource {
    /// OCR block behind the line (screenshots), if any.
    pub block_id: Option<String>,
    /// screenshot | audio | caption | keyframe
    pub kind: &'static str,
    /// Moment in the Reel.
    pub time_sec: Option<f64>,
}

impl LineSource {
    pub fn block(id: &str) -> Self {
        Self { block_id: Some(id.to_string()), kind: "screenshot", time_sec: None }
    }
}

pub struct AttachRequest<'a> {
    pub candidate: &'a PlaceCandidate,
    pub extracted: Option<&'a ExtractedPlace>,
    pub evidence: Evidence<'a>,
    pub blocks: &'a [OcrBlockRecord],
    /// `line_sources[i]` = where AI line i came from.
    pub line_sources: &'a [LineSource],
    pub confidence: f64,
    pub verification: Verification,
    pub origin: DataOrigin,
}

#[derive(Debug, Clone)]
pub struct AttachOutcome {
    pub place_id: String,
    pub created: bool,
    pub possible_duplicate_of: Option<String>,
}

/// Adds a screenshot or Reel (and its facts) to the matching place, or creates the place.
pub fn attach(db: &Database, req: AttachRequest) -> Result<AttachOutcome> {
    let extracted_names = req.extracted.map(|e| e.names()).unwrap_or_default();
    let category = req
        .extracted
        .map(|e| e.place_category())
        .filter(|c| *c != PlaceCategory::Other)
        .or_else(|| req.candidate.category_hint())
        .unwrap_or(PlaceCategory::Other);

    let (place, created, possible_duplicate_of) = match find_match(db, req.candidate, &extracted_names)? {
        PlaceMatch::Same(existing) => {
            if !existing.is_user_verified {
                if existing.category == PlaceCategory::Other && category != PlaceCategory::Other {
                    db.update_place_field(&existing.id, "category", Some(category.as_str()))?;
                }
                if existing.verification == Verification::NeedsReview && req.verification != Verification::NeedsReview {
                    db.set_place_verification(&existing.id, req.verification)?;
                }
            }
            (existing, false, None)
        }
        PlaceMatch::Possible(other) => {
            let p = db.insert_place(req.candidate, category, req.verification, req.origin, &[])?;
            (p, true, Some(other.id))
        }
        PlaceMatch::None => (db.insert_place(req.candidate, category, req.verification, req.origin, &[])?, true, None),
    };

    // Alternative/localised names seen in sources ("東京スカイツリー", "Tokyo Sky Tree").
    let mut alt = place.alternative_names.clone();
    for n in &extracted_names {
        if name_similarity(n, &place.canonical_name) < 0.95 && !alt.iter().any(|a| name_similarity(a, n) >= 0.95) {
            alt.push(n.clone());
        }
    }
    if alt != place.alternative_names {
        db.set_alternative_names(&place.id, &alt)?;
    }

    let extracted_name = req.extracted.map(|e| e.display_name.clone()).unwrap_or_else(|| req.candidate.name.clone());
    let (screenshot_id, reel_id, valid_from, default_kind) = match req.evidence {
        Evidence::Screenshot(s) => {
            db.link(&place.id, &s.id, &extracted_name, req.confidence, req.origin)?;
            (Some(s.id.as_str()), None, s.creation_date.as_deref(), "screenshot")
        }
        Evidence::Reel(r) => {
            db.link_reel(&place.id, &r.id, &extracted_name, req.confidence, req.origin)?;
            (None, Some(r.id.as_str()), r.posted_at.as_deref().or(Some(r.created_at.as_str())), "caption")
        }
    };

    if let Some(extracted) = req.extracted {
        let mut existing = db.facts_for_place(&place.id)?;
        for fact in extracted.all_facts() {
            let kind = fact.kind();
            if is_duplicate_fact(&existing, kind, &fact.text) {
                continue;
            }
            let lines = fact.source_lines.clone().unwrap_or_default();
            let sources: Vec<&LineSource> = lines.iter().filter_map(|l| usize::try_from(*l).ok().and_then(|i| req.line_sources.get(i))).collect();
            let block_ids = if screenshot_id.is_some() {
                let line_ids: Vec<String> = req.line_sources.iter().map(|s| s.block_id.clone().unwrap_or_default()).collect();
                fact_block_ids(&lines, fact.source_quote.as_deref(), &line_ids, req.blocks)
            } else {
                vec![]
            };
            let provenance = FactProvenance {
                screenshot_id,
                reel_id,
                kind: sources.first().map(|s| s.kind).unwrap_or(default_kind),
                time_sec: sources.iter().find_map(|s| s.time_sec),
                block_ids,
                valid_from,
            };
            let id = db.insert_fact(&place.id, kind, fact.text.trim(), fact.source_quote.as_deref(), fact.confidence.unwrap_or(0.7), &provenance, DataOrigin::Ai)?;
            existing.push(FactRecord {
                id, place_id: place.id.clone(), screenshot_id: screenshot_id.map(Into::into), reel_id: reel_id.map(Into::into),
                source_kind: provenance.kind.into(), source_time_sec: provenance.time_sec, fact_type: kind, text: fact.text.clone(),
                source_quote: None, confidence: 0.0, source_block_ids: vec![], valid_from: None, origin: DataOrigin::Ai,
                created_at: String::new(), source_type: None, creator: None,
            });
        }
    }
    Ok(AttachOutcome { place_id: place.id, created, possible_duplicate_of })
}

/// Maps a fact back to OCR blocks: the AI's line numbers first, else a quote match.
pub fn fact_block_ids(lines: &[i64], quote: Option<&str>, line_block_ids: &[String], blocks: &[OcrBlockRecord]) -> Vec<String> {
    let mut ids: Vec<String> = lines
        .iter()
        .filter_map(|l| usize::try_from(*l).ok().and_then(|i| line_block_ids.get(i)).cloned())
        .collect();
    if ids.is_empty() {
        if let Some(q) = quote.map(normalize).filter(|q| q.len() >= 4) {
            ids = blocks
                .iter()
                .filter(|b| {
                    let t = normalize(&b.text);
                    t.len() >= 4 && (q.contains(&t) || t.contains(&q))
                })
                .map(|b| b.id.clone())
                .collect();
        }
    }
    ids.dedup();
    ids
}

/// User action: merge `source` into `target`, keeping every screenshot, fact, photo and trip entry.
pub fn merge_places(db: &Database, source_id: &str, target_id: &str) -> Result<()> {
    anyhow::ensure!(source_id != target_id, "cannot merge a place into itself");
    let source = db.place(source_id)?.ok_or_else(|| anyhow::anyhow!("place not found"))?;
    let target = db.place(target_id)?.ok_or_else(|| anyhow::anyhow!("place not found"))?;
    let mut alt = target.alternative_names.clone();
    for n in source.all_names() {
        if name_similarity(&n, &target.canonical_name) < 0.95 && !alt.contains(&n) {
            alt.push(n);
        }
    }
    db.transaction(|tx| {
        tx.execute(
            "INSERT OR IGNORE INTO place_screenshots(place_id, screenshot_id, extracted_name, confidence, origin, is_user_verified, created_at) \
             SELECT ?2, screenshot_id, extracted_name, confidence, origin, is_user_verified, created_at FROM place_screenshots WHERE place_id = ?1",
            params![source_id, target_id],
        )?;
        tx.execute(
            "INSERT OR IGNORE INTO place_reels(place_id, reel_id, extracted_name, confidence, origin, is_user_verified, created_at) \
             SELECT ?2, reel_id, extracted_name, confidence, origin, is_user_verified, created_at FROM place_reels WHERE place_id = ?1",
            params![source_id, target_id],
        )?;
        tx.execute("UPDATE travel_facts SET place_id = ?2 WHERE place_id = ?1", params![source_id, target_id])?;
        tx.execute("UPDATE place_images SET place_id = ?2 WHERE place_id = ?1", params![source_id, target_id])?;
        tx.execute(
            "INSERT OR IGNORE INTO trip_places(id, trip_id, place_id, position, day) SELECT id || '-m', trip_id, ?2, position, day FROM trip_places WHERE place_id = ?1",
            params![source_id, target_id],
        )?;
        tx.execute("UPDATE places SET alternative_names = ?2, updated_at = ?3 WHERE id = ?1",
                   params![target_id, serde_json::to_string(&alt).unwrap(), crate::db::now()])?;
        tx.execute("UPDATE review_items SET is_resolved = 1, resolution = 'merged' WHERE place_a_id IN (?1, ?2) AND place_b_id IN (?1, ?2)",
                   params![source_id, target_id])?;
        tx.execute("DELETE FROM places WHERE id = ?1", [source_id])?;
        Ok(())
    })
}

/// User action: move some sources (screenshot or Reel ids, with their facts and photos) out of a wrongly merged place.
pub fn split_place(db: &Database, place_id: &str, source_ids: &[String], new_location: &PlaceCandidate) -> Result<String> {
    let original = db.place(place_id)?.ok_or_else(|| anyhow::anyhow!("place not found"))?;
    let new_place = db.insert_place(new_location, original.category, Verification::UserVerified, DataOrigin::User, &[])?;
    db.transaction(|tx| {
        for sid in source_ids {
            tx.execute(
                "UPDATE place_reels SET place_id = ?3, origin = 'user', is_user_verified = 1 WHERE place_id = ?1 AND reel_id = ?2",
                params![place_id, sid, new_place.id],
            )?;
            tx.execute("UPDATE travel_facts SET place_id = ?3 WHERE place_id = ?1 AND reel_id = ?2", params![place_id, sid, new_place.id])?;
            tx.execute("UPDATE place_images SET place_id = ?3 WHERE place_id = ?1 AND reel_id = ?2", params![place_id, sid, new_place.id])?;
            tx.execute(
                "UPDATE place_screenshots SET place_id = ?3, origin = 'user', is_user_verified = 1 WHERE place_id = ?1 AND screenshot_id = ?2",
                params![place_id, sid, new_place.id],
            )?;
            tx.execute("UPDATE travel_facts SET place_id = ?3 WHERE place_id = ?1 AND screenshot_id = ?2", params![place_id, sid, new_place.id])?;
            tx.execute("UPDATE place_images SET place_id = ?3 WHERE place_id = ?1 AND screenshot_id = ?2", params![place_id, sid, new_place.id])?;
        }
        Ok(())
    })?;
    Ok(new_place.id)
}
