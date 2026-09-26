//! Instagram Reel pipeline:
//! URL → metadata + video → audio transcript (on-device) → caption → keyframe OCR (Vision)
//! → ONE DeepSeek call → place resolution → merge into existing places (with timestamps as provenance).

use std::path::PathBuf;
use std::time::Instant;

use anyhow::{anyhow, Result};

use super::merge::{attach, AttachRequest, Evidence, LineSource};
use super::{compress_for_ai, regions, Pipeline, Stop};
use crate::config::{AppConfig, AI_PROMPT_VERSION};
use crate::db::{ReelRecord, TranscriptSegment};
use crate::models::*;
use crate::services::ai::local_filter;
use crate::services::ai::types::*;
use crate::services::native::{SpeechLocale, Transcription, WordTiming};
use crate::services::reels::{is_instagram_url, shortcode};
use crate::text::normalize;

/// Groups word timings into readable, timestamped transcript lines.
pub fn group_words(words: &[WordTiming]) -> Vec<TranscriptSegment> {
    let mut out: Vec<TranscriptSegment> = Vec::new();
    let mut current: Option<TranscriptSegment> = None;
    let mut last_end = 0.0;
    for w in words {
        let text = w.text.trim();
        if text.is_empty() {
            continue;
        }
        let end = w.start + w.duration.max(0.0);
        let break_here = match &current {
            Some(seg) => w.start - last_end > 0.8 || seg.end - seg.start > 8.0 || seg.text.ends_with(['.', '!', '?']),
            None => false,
        };
        if break_here {
            out.extend(current.take());
        }
        match current.as_mut() {
            Some(seg) => {
                seg.text.push(' ');
                seg.text.push_str(text);
                seg.end = end;
            }
            None => current = Some(TranscriptSegment { start: w.start, end, text: text.to_string() }),
        }
        last_end = end;
    }
    out.extend(current);
    out
}

/// The numbered text evidence for one Reel, in priority order: audio, caption, on-screen text.
pub fn build_reel_lines(caption: Option<&str>, transcript: &[TranscriptSegment], frames: &[(f64, Vec<String>)]) -> (Vec<String>, Vec<LineSource>) {
    let mut lines = Vec::new();
    let mut sources = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut push = |text: String, key: String, source: LineSource, lines: &mut Vec<String>, sources: &mut Vec<LineSource>| {
        if seen.insert(key) {
            lines.push(text);
            sources.push(source);
        }
    };

    for seg in transcript {
        push(format!("audio {}: {}", mmss(seg.start), seg.text), normalize(&seg.text), LineSource { block_id: None, kind: "audio", time_sec: Some(seg.start) }, &mut lines, &mut sources);
    }
    if let Some(caption) = caption {
        for l in caption.lines().map(str::trim).filter(|l| l.chars().count() > 2).take(40) {
            push(format!("caption: {l}"), normalize(l), LineSource { block_id: None, kind: "caption", time_sec: None }, &mut lines, &mut sources);
        }
    }
    // On-screen text: only lines not already said or captioned, UI chrome removed, deduped across frames.
    for (time, texts) in frames {
        for i in local_filter::content_lines(texts) {
            let key = normalize(&texts[i]);
            if key.len() < 3 {
                continue;
            }
            push(format!("on-screen {}: {}", mmss(*time), texts[i]), key, LineSource { block_id: None, kind: "keyframe", time_sec: Some(*time) }, &mut lines, &mut sources);
        }
    }
    (lines, sources)
}

/// Below this mean word confidence a transcript is treated as music/lyrics rather than speech.
pub const MIN_SPEECH_CONFIDENCE: f64 = 0.7;

/// Stages shown for every Reel, in display order.
pub const STAGES: [&str; 8] = ["caption", "video", "audio", "transcript", "keyframes", "ocr", "ai", "places"];

/// Default speech locale for a detected language (BCP-47 language → locale).
const DEFAULT_LOCALES: &[(&str, &str)] = &[
    ("en", "en-US"), ("ja", "ja-JP"), ("zh-Hans", "zh-CN"), ("zh-Hant", "zh-TW"), ("zh", "zh-CN"), ("yue", "yue-CN"),
    ("ko", "ko-KR"), ("ta", "ta-IN"), ("si", "si-LK"), ("hi", "hi-IN"), ("bn", "bn-IN"), ("th", "th-TH"),
    ("vi", "vi-VN"), ("id", "id-ID"), ("ms", "ms-MY"), ("fr", "fr-FR"), ("de", "de-DE"), ("es", "es-ES"),
    ("it", "it-IT"), ("pt", "pt-BR"), ("ru", "ru-RU"), ("ar", "ar-SA"), ("tr", "tr-TR"), ("nl", "nl-NL"),
];

fn locale_for_language(lang: &str, supported: &[SpeechLocale]) -> Option<String> {
    let base = lang.split('-').next().unwrap_or(lang);
    let preferred = DEFAULT_LOCALES.iter().find(|(l, _)| *l == lang).or_else(|| DEFAULT_LOCALES.iter().find(|(l, _)| *l == base)).map(|(_, loc)| loc.to_string());
    if supported.is_empty() {
        return preferred;
    }
    if let Some(p) = preferred.filter(|p| supported.iter().any(|s| s.id.eq_ignore_ascii_case(p))) {
        return Some(p);
    }
    // Any supported locale of the same language, preferring on-device ones.
    let mut same: Vec<&SpeechLocale> = supported.iter().filter(|s| s.id.split('-').next() == Some(base)).collect();
    same.sort_by_key(|s| !s.on_device);
    same.first().map(|s| s.id.clone())
}

/// Locales to try, in order. A specific choice is used as-is (the bridge explains if unsupported);
/// "auto" uses language hints from the caption and on-screen text, then English.
pub fn candidate_locales(requested: &str, hints: &[(String, f64)], supported: &[SpeechLocale]) -> Vec<String> {
    if !requested.eq_ignore_ascii_case("auto") && !requested.is_empty() {
        return vec![requested.to_string()];
    }
    let mut out: Vec<String> = Vec::new();
    // Only confident hints; noisy on-screen text otherwise sends Auto to the wrong language first.
    for (lang, p) in hints {
        if *p >= 0.8 {
            if let Some(loc) = locale_for_language(lang, supported) {
                if !out.contains(&loc) { out.push(loc); }
            }
        }
    }
    if let Some(en) = locale_for_language("en", supported) {
        if !out.contains(&en) { out.push(en); }
    }
    out
}

/// Confidence weighted by how much speech was recognised.
fn transcript_score(t: &Transcription) -> f64 {
    t.confidence() * (t.words.len().min(10) as f64 / 10.0)
}

fn mmss(t: f64) -> String {
    let s = t.max(0.0) as u64;
    format!("{:02}:{:02}", s / 60, s % 60)
}

impl Pipeline {
    fn reel_dir(&self, id: &str) -> PathBuf {
        self.data_dir.join("reels").join(id)
    }

    /// Registers a pasted Instagram URL (same shortcode ⇒ same Reel). Processing is started by the caller.
    pub fn register_reel(&self, url: &str) -> Result<(String, bool)> {
        let url = url.trim();
        if !is_instagram_url(url) {
            return Err(anyhow!("That doesn't look like an Instagram Reel or post link."));
        }
        self.db.insert_reel(url, shortcode(url).as_deref())
    }

    /// Fallback: use a video from Photos (for a new Reel or an existing one without media).
    pub async fn attach_photos_video(&self, reel_id: Option<&str>, asset_id: &str) -> Result<String> {
        let id = match reel_id {
            Some(id) => id.to_string(),
            None => self.db.insert_reel(&format!("photos://{asset_id}"), None)?.0,
        };
        let out = self.reel_dir(&id).join("video.mp4");
        let created = self.media.export_video(asset_id, &out).await?;
        self.db.set_reel_media(&id, &out.to_string_lossy(), "photos", None)?;
        if let Some(date) = created {
            self.db.set_reel_metadata(&id, None, None, Some(&date), None)?;
        }
        self.db.clear_reel_derived(&id)?;
        Ok(id)
    }

    /// Re-transcribe with a specific locale ("auto" to detect again), then update the extraction.
    pub async fn retranscribe_reel(&self, id: &str, locale: &str) -> ProcessingStatus {
        let _ = self.db.set_transcript_override(id, Some(locale));
        self.reprocess_reel(id).await
    }

    pub async fn reprocess_reel(&self, id: &str) -> ProcessingStatus {
        let _ = self.db.clear_reel_derived(id);
        self.process_reel(id).await
    }

    /// Processes one Reel end to end; a failure affects only this Reel.
    pub async fn process_reel(&self, id: &str) -> ProcessingStatus {
        let start = Instant::now();
        let status = match self.run_reel(id).await {
            Ok(s) => s,
            Err(Stop::Waiting(detail)) => {
                let _ = self.db.set_reel_status(id, ProcessingStatus::WaitingForNetwork, Some(&detail));
                ProcessingStatus::WaitingForNetwork
            }
            Err(Stop::Failed(message)) => {
                log::warn!(target: "reels", "reel {id} failed: {message}");
                let _ = self.db.record_reel_failure(id, &message);
                ProcessingStatus::Failed
            }
        };
        self.time("reels.total", start);
        self.notify();
        status
    }

    async fn run_reel(&self, id: &str) -> Result<ProcessingStatus, Stop> {
        let fail = |e: anyhow::Error| Stop::Failed(e.to_string());
        let config = self.config();
        let mut reel = self.db.reel(id).map_err(fail)?.ok_or_else(|| Stop::Failed("Reel not found".into()))?;
        let dir = self.reel_dir(id);
        self.db.reset_reel_stages(id).map_err(fail)?;
        for s in STAGES {
            let _ = self.db.set_reel_stage(id, s, "pending", None);
        }
        // Every stage records its outcome and duration; a failed stage never stops the stages after it.
        let started: std::sync::Mutex<std::collections::HashMap<String, Instant>> = Default::default();
        let stage = |name: &str, status: &str, detail: Option<String>| {
            let detail = if status == "running" {
                started.lock().unwrap().insert(name.to_string(), Instant::now());
                detail
            } else {
                let took = started.lock().unwrap().remove(name).map(|t| format!("{:.1} s", t.elapsed().as_secs_f64()));
                match (detail, took) {
                    (Some(d), Some(t)) => Some(format!("{d} · {t}")),
                    (None, t) => t,
                    (d, None) => d,
                }
            };
            let _ = self.db.set_reel_stage(id, name, status, detail.as_deref());
            self.notify();
        };
        let short = |e: &dyn std::fmt::Display| e.to_string().chars().take(160).collect::<String>();

        // 1–2. Caption/metadata and the video — a single yt-dlp call when available.
        let had_media = reel.media_path.as_ref().is_some_and(|p| std::path::Path::new(p).exists());
        self.db.set_reel_status(id, ProcessingStatus::Loading, Some("Fetching Reel")).map_err(fail)?;
        if reel.url.starts_with("http") {
            stage("caption", "running", None);
            if !had_media {
                stage("video", "running", None);
            }
            let video = dir.join("video.mp4");
            let fetched = if had_media {
                self.fetcher.metadata(&reel.url, &config).await.map(|m| (m, None))
            } else {
                self.fetcher.fetch(&reel.url, &video, &config).await
            };
            match fetched {
                Ok((meta, source)) => {
                    self.db.set_reel_metadata(id, meta.creator.as_deref(), meta.caption.as_deref(), meta.posted_at.as_deref(), None).map_err(fail)?;
                    match &meta.caption {
                        Some(c) => stage("caption", "done", Some(format!("{} characters{}", c.chars().count(),
                            meta.creator.as_ref().map(|h| format!(" · {h}")).unwrap_or_default()))),
                        None => stage("caption", "skipped", Some("No caption available".into())),
                    }
                    if had_media {
                        stage("video", "done", Some("Already saved".into()));
                    } else if let Some(source) = source {
                        self.db.set_reel_media(id, &video.to_string_lossy(), &source, meta.duration_sec).map_err(fail)?;
                        stage("video", "done", Some(if source == "ytdlp" { "Downloaded with yt-dlp".into() } else { "Downloaded from the page".into() }));
                    } else {
                        let why = if self.fetcher.yt_dlp(&config).is_none() {
                            "Instagram didn't provide the video (install yt-dlp, or import it from Photos)"
                        } else {
                            "Instagram didn't provide the video (try browser cookies in Settings, or import it from Photos)"
                        };
                        stage("video", "failed", Some(why.into()));
                        if let Some(thumb) = &meta.thumbnail_url {
                            let cover = dir.join("cover.jpg");
                            if self.fetcher.download_file(thumb, &cover).await.is_ok() {
                                let _ = self.db.set_reel_metadata(id, None, None, None, Some(&cover.to_string_lossy()));
                            }
                        }
                    }
                }
                Err(e) => {
                    let msg = e.to_string().to_lowercase();
                    let offline = msg.contains("connect") || msg.contains("dns") || msg.contains("offline") || msg.contains("network");
                    stage("caption", "failed", Some(if offline { "Offline".into() } else { short(&e) }));
                    if !had_media {
                        stage("video", "failed", Some(if offline { "Offline".into() } else { short(&e) }));
                    }
                    if offline {
                        return Err(Stop::Waiting(format!("Offline: {}", short(&e))));
                    }
                }
            }
        } else {
            stage("caption", "skipped", Some("Imported from Photos — no caption".into()));
            stage("video", if had_media { "done" } else { "failed" }, Some("Video from Photos".into()));
        }
        reel = self.db.reel(id).map_err(fail)?.unwrap_or(reel);
        let media = reel.media_path.clone().filter(|p| std::path::Path::new(p).exists()).map(PathBuf::from);

        // 3–6. Audio → transcript and key snapshots → on-screen text run in parallel.
        // The spoken language is hinted by the caption; without one, the on-screen text is read first.
        let requested = reel.transcript_locale_override.clone().filter(|l| !l.is_empty())
            .unwrap_or_else(|| config.transcription_locale.clone());
        let caption_hint: String = reel.caption.clone().unwrap_or_default().split_whitespace()
            .filter(|w| !w.starts_with('#') && !w.starts_with('@')).collect::<Vec<_>>().join(" ");
        let (transcript, frames_text) = match &media {
            Some(video) => {
                self.db.set_reel_status(id, ProcessingStatus::OcrProcessing, Some("Reading audio and key frames")).map_err(fail)?;
                if caption_hint.chars().count() >= 20 || !requested.eq_ignore_ascii_case("auto") {
                    tokio::join!(
                        self.reel_audio(id, video, &dir, &requested, &caption_hint, &stage),
                        self.reel_frames(id, video, &dir, config.max_keyframes, &stage),
                    )
                } else {
                    let frames = self.reel_frames(id, video, &dir, config.max_keyframes, &stage).await;
                    let hint = frames.iter().flat_map(|(_, t)| t.clone()).collect::<Vec<_>>().join(" ");
                    (self.reel_audio(id, video, &dir, &requested, &hint, &stage).await, frames)
                }
            }
            None => {
                for s in ["audio", "transcript", "keyframes", "ocr"] {
                    stage(s, "skipped", Some("No video".into()));
                }
                (Vec::new(), Vec::new())
            }
        };
        reel = self.db.reel(id).map_err(fail)?.unwrap_or(reel);

        // 7. One DeepSeek call on text evidence (audio → caption → on-screen text).
        let (lines, line_sources) = build_reel_lines(reel.caption.as_deref(), &transcript, &frames_text);
        if lines.is_empty() {
            stage("ai", "skipped", Some("Nothing to analyse".into()));
            stage("places", "skipped", None);
            let (status, detail) = if media.is_none() {
                (ProcessingStatus::NeedsMedia, "Instagram didn't share this Reel's video or caption. Import the video from Photos to analyse it.")
            } else {
                (ProcessingStatus::NotTravel, "No speech, caption or on-screen text found.")
            };
            self.db.finish_reel(id, status, Some(detail)).map_err(fail)?;
            return Ok(status);
        }
        self.db.set_reel_status(id, ProcessingStatus::Extracting, Some("Understanding the Reel")).map_err(fail)?;
        stage("ai", "running", None);
        let text_len: usize = lines.iter().map(|l| l.len()).sum();
        let cover = self.db.keyframes(id).ok().and_then(|k| k.into_iter().find(|f| f.is_cover));
        let image = if text_len < 60 && config.allow_vision_requests {
            // Only when the text evidence is too thin: one compressed cover frame, never the video.
            cover.as_ref().and_then(|c| image::open(&c.image_path).ok()).and_then(|img| compress_for_ai(&img.to_rgb8(), config.vision_image_max_pixel_size))
        } else {
            None
        };
        let input = ScreenshotAiInput {
            lines,
            line_block_ids: vec![],
            creation_date: reel.posted_at.clone(),
            source_hint: Some("instagram_reel".into()),
            creator_hint: reel.creator.clone(),
            image_jpeg: image,
        };
        let extraction = match self.extract_reel(&reel, &input, &config).await {
            Ok(e) => e,
            Err(err) => {
                let msg = match &err { Stop::Waiting(m) | Stop::Failed(m) => m.clone() };
                stage("ai", "failed", Some(msg.chars().take(160).collect()));
                stage("places", "skipped", None);
                return Err(err);
            }
        };
        let after = self.db.reel(id).ok().flatten();
        stage("ai", "done", Some(format!("{} place(s) · {} text lines{}{}", extraction.all_places().len(), input.lines.len(),
            if input.image_jpeg.is_some() { " · 1 image" } else { "" },
            after.map(|r| format!(" · ${:.5}", r.ai_cost)).unwrap_or_default())));

        // An imported Reel was saved on purpose: travel unless the AI clearly disagrees.
        if !(extraction.is_travel_related || !extraction.all_places().is_empty()) {
            stage("places", "skipped", Some("Not travel-related".into()));
            self.db.finish_reel(id, ProcessingStatus::NotTravel, extraction.reason.as_deref()).map_err(fail)?;
            return Ok(ProcessingStatus::NotTravel);
        }

        // 8. Resolve each place independently and merge with existing places.
        self.db.set_reel_status(id, ProcessingStatus::ResolvingPlaces, Some("Finding places")).map_err(fail)?;
        stage("places", "running", None);
        let user_links = self.db.user_reel_links(id).map_err(fail)?;
        let user_names: Vec<String> = user_links.iter().map(|(_, n)| n.clone()).collect();
        let mut linked: Vec<(String, bool)> = user_links.iter().map(|(p, _)| (p.clone(), false)).collect();
        match self.resolve_places(Evidence::Reel(&reel), &extraction, &[], &line_sources, &user_names, &config).await {
            Ok(l) => linked.extend(l),
            Err(err) => {
                let msg = match &err { Stop::Waiting(m) | Stop::Failed(m) => m.clone() };
                stage("places", "failed", Some(msg));
                return Err(err);
            }
        }
        let reviews = self.db.reviews_for_reel(id).map_err(fail)?.iter().filter(|r| !r.is_resolved).count();
        let r = self.db.reel(id).ok().flatten();
        stage("places", "done", Some(format!("{} on the map{}",
            r.as_ref().map(|r| r.places_auto_resolved).unwrap_or(0),
            if reviews > 0 { format!(" · {reviews} to review") } else { String::new() })));

        if let Err(e) = self.reel_photo(&reel, &linked, &config).await {
            log::warn!(target: "images", "reel photo extraction failed: {e}");
        }

        let (status, detail) = if reviews > 0 {
            (ProcessingStatus::NeedsReview, None)
        } else if media.is_none() && linked.is_empty() {
            (ProcessingStatus::NeedsMedia, Some("Only the caption was available. Import the video from Photos for more."))
        } else if media.is_none() {
            (ProcessingStatus::Complete, Some("From the caption only — import the video from Photos for more."))
        } else {
            (ProcessingStatus::Complete, None)
        };
        self.db.finish_reel(id, status, detail).map_err(fail)?;
        Ok(status)
    }

    /// Audio branch: save the voice track, then transcribe it (Auto language or the chosen one).
    async fn reel_audio(&self, id: &str, video: &std::path::Path, dir: &std::path::Path, requested: &str, hint: &str,
                        stage: &(dyn Fn(&str, &str, Option<String>) + Sync)) -> Vec<TranscriptSegment> {
        stage("audio", "running", None);
        let audio = match self.media.extract_audio(video, &dir.join("audio.m4a")).await {
            Ok((Some(path), duration)) => {
                let source = self.db.reel(id).ok().flatten().and_then(|r| r.media_source).unwrap_or_else(|| "photos".into());
                let _ = self.db.set_reel_media(id, &video.to_string_lossy(), &source, Some(duration));
                stage("audio", "done", Some(format!("{duration:.0} s of audio saved")));
                path
            }
            Ok((None, _)) => {
                stage("audio", "skipped", Some("The video has no audio track".into()));
                stage("transcript", "skipped", Some("No audio".into()));
                return Vec::new();
            }
            Err(e) => {
                stage("audio", "failed", Some(e.to_string().chars().take(160).collect()));
                stage("transcript", "skipped", Some("No audio".into()));
                return Vec::new();
            }
        };

        stage("transcript", "running", None);
        let t = Instant::now();
        let result = self.transcribe_best(&audio, requested, hint).await;
        self.time("reels.transcribe", t);
        match result {
            Ok((tr, tried)) => {
                let transcript = group_words(&tr.words);
                let label = format!("{}{}", tr.locale, if tr.on_device { ", on-device" } else { ", Apple server" });
                // Sung lyrics come back with low confidence (measured ~60% vs ~97% for speech): keep them
                // visible but leave them out of the AI input.
                let music = !tr.words.is_empty() && tr.confidence() < MIN_SPEECH_CONFIDENCE;
                let _ = self.db.set_reel_transcript(id, Some(&audio.to_string_lossy()), &transcript, Some(&label));
                let _ = self.db.set_transcript_confidence(id, (!tr.words.is_empty()).then(|| tr.confidence()));
                let detail = if transcript.is_empty() {
                    format!("No speech detected (tried {})", tried.join(", "))
                } else if music {
                    format!("{} lines · likely music or lyrics (confidence {:.0}%) — not used for places", transcript.len(), tr.confidence() * 100.0)
                } else {
                    format!("{} lines · {} · confidence {:.0}%{}", transcript.len(), label, tr.confidence() * 100.0,
                            if tried.len() > 1 { format!(" · tried {}", tried.join(", ")) } else { String::new() })
                };
                stage("transcript", "done", Some(detail));
                if music { Vec::new() } else { transcript }
            }
            Err(e) => {
                let _ = self.db.set_reel_transcript(id, Some(&audio.to_string_lossy()), &[], None);
                stage("transcript", "failed", Some(e.to_string().chars().take(160).collect()));
                Vec::new()
            }
        }
    }

    /// Visual branch: a few distinct key snapshots, read with Vision OCR concurrently (never every frame).
    async fn reel_frames(&self, id: &str, video: &std::path::Path, dir: &std::path::Path, max_frames: u32,
                         stage: &(dyn Fn(&str, &str, Option<String>) + Sync)) -> Vec<(f64, Vec<String>)> {
        stage("keyframes", "running", None);
        let t = Instant::now();
        let frames_dir = dir.join("frames");
        let _ = std::fs::remove_dir_all(&frames_dir);
        let frames = match self.media.keyframes(video, &frames_dir, max_frames).await {
            Ok((_, frames)) => {
                stage("keyframes", "done", Some(format!("{} key snapshots", frames.len())));
                frames
            }
            Err(e) => {
                stage("keyframes", "failed", Some(e.to_string().chars().take(160).collect()));
                stage("ocr", "skipped", Some("No key snapshots".into()));
                return Vec::new();
            }
        };

        stage("ocr", "running", None);
        let results = futures::future::join_all(frames.iter().map(|f| self.ocr.recognize(std::path::Path::new(&f.path)))).await;
        let (mut rows, mut out, mut errors, mut lines) = (Vec::new(), Vec::new(), 0, 0);
        for (f, result) in frames.iter().zip(results) {
            let blocks = result.unwrap_or_else(|_| { errors += 1; vec![] });
            let ordered = super::reading_order(&blocks);
            let texts: Vec<String> = ordered.iter().map(|b| b.text.clone()).collect();
            lines += texts.len();
            let blocks_json = serde_json::to_string(&ordered.iter().map(|b| serde_json::json!({
                "text": b.text, "confidence": b.confidence, "x": b.x, "y": b.y, "width": b.width, "height": b.height
            })).collect::<Vec<_>>()).unwrap_or_else(|_| "[]".into());
            rows.push((f.time_sec, f.path.clone(), texts.join("\n"), blocks_json));
            out.push((f.time_sec, texts));
        }
        let _ = self.db.replace_keyframes(id, &rows);
        if !rows.is_empty() && errors == rows.len() {
            stage("ocr", "failed", Some("Text recognition failed on every frame".into()));
        } else {
            stage("ocr", "done", Some(format!("{lines} lines of on-screen text")));
        }
        self.time("reels.keyframes", t);
        out
    }

    /// Transcribes with the requested locale, or — for "auto" — tries the likely locales (from
    /// caption/on-screen language hints, then English) and keeps the most confident transcript.
    async fn transcribe_best(&self, audio: &std::path::Path, requested: &str, hint_text: &str) -> Result<(Transcription, Vec<String>)> {
        let auto = requested.eq_ignore_ascii_case("auto");
        let supported = self.media.speech_locales().await.unwrap_or_default();
        let hints = if auto && !hint_text.trim().is_empty() { self.media.detect_language(hint_text).await.unwrap_or_default() } else { vec![] };
        let candidates = candidate_locales(requested, &hints, &supported);
        let mut best: Option<Transcription> = None;
        let mut tried = Vec::new();
        let mut last_error = None;
        for locale in candidates.iter().take(3) {
            tried.push(locale.clone());
            match self.media.transcribe(audio, locale).await {
                Ok(tr) => {
                    let good = tr.words.len() >= 3 && tr.confidence() >= 0.6;
                    if best.as_ref().is_none_or(|b| transcript_score(&tr) > transcript_score(b)) {
                        best = Some(tr);
                    }
                    if good || !auto {
                        break;
                    }
                }
                Err(e) => last_error = Some(e),
            }
        }
        match best {
            Some(b) => Ok((b, tried)),
            None => Err(last_error.unwrap_or_else(|| anyhow!("No speech recognition language available"))),
        }
    }

    async fn extract_reel(&self, reel: &ReelRecord, input: &ScreenshotAiInput, config: &AppConfig) -> Result<TravelExtraction, Stop> {
        let ai = self.ai();
        let model = ai.model_name();
        let hash = crate::text::stable_hash(&format!("reel\n{}", input.lines.join("\n")));
        if let Ok(Some(json)) = self.db.ai_cache_get(&hash, &model, AI_PROMPT_VERSION, "reel") {
            if let Ok(cached) = decode_json::<TravelExtraction>(&json) {
                self.db.add_metric("ai.cacheHits", 1.0);
                return Ok(cached);
            }
        }
        if config.daily_ai_request_limit > 0 && self.db.ai_requests_today().unwrap_or(0) >= config.daily_ai_request_limit as i64 {
            return Err(Stop::Waiting(AiError::DailyLimitReached.to_string()));
        }
        let t = Instant::now();
        // No thinking escalation for Reels: caption + transcript + on-screen text already give rich context, and
        // ambiguous names are settled by map scoring and the Review inbox (measured: +23 s, ~6x cost, same result).
        let outcome = ai.extract_travel_information(input, config.use_thinking_by_default).await;
        self.time("ai.reel", t);
        match outcome {
            Ok(r) => {
                self.db.log_ai_usage(None, "reel", &r.usage, true, AI_PROMPT_VERSION);
                let cls = if r.value.is_travel_related { Classification::Travel } else { Classification::NotTravel };
                let _ = self.db.set_reel_ai(&reel.id, cls, r.value.travel_confidence, &hash, &model, AI_PROMPT_VERSION,
                                            r.usage.input_tokens, r.usage.output_tokens, r.usage.estimated_cost);
                if reel.creator.is_none() {
                    let _ = self.db.set_reel_metadata(&reel.id, r.value.creator.as_deref(), None, None, None);
                }
                let _ = self.db.ai_cache_put(&hash, &model, AI_PROMPT_VERSION, "reel", &reel.id, &r.raw_json);
                Ok(r.value)
            }
            Err(e) => {
                let usage = AiUsage { model, image_used: input.image_jpeg.is_some(), ..Default::default() };
                self.db.log_ai_usage(None, "reel", &usage, false, AI_PROMPT_VERSION);
                if e.is_transient() { Err(Stop::Waiting(e.to_string())) } else { Err(Stop::Failed(e.to_string())) }
            }
        }
    }

    async fn reel_photo(&self, reel: &ReelRecord, linked: &[(String, bool)], config: &AppConfig) -> Result<()> {
        let target = if linked.len() == 1 {
            Some(linked[0].0.clone())
        } else {
            let flagged: Vec<_> = linked.iter().filter(|(_, useful)| *useful).collect();
            (flagged.len() == 1).then(|| flagged[0].0.clone())
        };
        let Some(place_id) = target else { return Ok(()) };
        let Some(cover) = self.db.keyframes(&reel.id)?.into_iter().find(|k| k.is_cover) else { return Ok(()) };
        let path = PathBuf::from(&cover.image_path);
        let Ok(img) = image::open(&path) else { return Ok(()) };
        let text_boxes: Vec<Rect> = vec![]; // cover frame is the one with the least on-screen text
        let regions = regions::detect(&img.to_rgb8(), &text_boxes, &[], SourceType::Instagram);
        let Some(best) = regions.into_iter().find(|r| r.region_type == RegionType::Photograph && r.confidence >= config.region_auto_accept_confidence) else {
            return Ok(());
        };
        let crop = self.data_dir.join("crops").join(format!("{}.jpg", crate::db::new_id()));
        self.photos.crop_image(&path, &crop, best.rect, 1600).await?;
        let print = self.vision.feature_print(&path, Some(best.rect)).await.ok();
        if let Some(print) = &print {
            for existing in self.db.images_for_place(&place_id)? {
                if existing.feature_print.as_ref().is_some_and(|o| regions::feature_distance(print, o) < config.duplicate_image_distance) {
                    let _ = std::fs::remove_file(&crop);
                    return Ok(());
                }
            }
        }
        let image_id = self.db.insert_image(&place_id, None, best.rect, &crop.to_string_lossy(), best.quality, RegionType::Photograph,
                                            best.confidence, print.as_deref(), true, DataOrigin::Local)?;
        self.db.set_image_reel(&image_id, &reel.id)?;
        Ok(())
    }

    /// Links a Reel to a place the user picked.
    pub fn user_attach_reel(&self, reel_id: &str, candidate: &PlaceCandidate, extracted: Option<&ExtractedPlace>) -> Result<String> {
        let reel = self.db.reel(reel_id)?.ok_or_else(|| anyhow!("Reel not found"))?;
        let outcome = attach(&self.db, AttachRequest {
            candidate, extracted, evidence: Evidence::Reel(&reel), blocks: &[], line_sources: &[], confidence: 1.0,
            verification: Verification::UserVerified, origin: DataOrigin::User,
        })?;
        self.db.set_place_user_verified(&outcome.place_id)?;
        Ok(outcome.place_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(text: &str, start: f64, duration: f64) -> WordTiming {
        WordTiming { text: text.into(), start, duration, confidence: 0.9 }
    }

    #[test]
    fn groups_words_into_timestamped_sentences() {
        let words = vec![
            w("This", 3.0, 0.2), w("is", 3.2, 0.2), w("Lake", 3.4, 0.3), w("Kawaguchi.", 3.7, 0.5),
            w("Come", 7.0, 0.2), w("early", 7.2, 0.3), w("for", 7.5, 0.1), w("clearer", 7.6, 0.3), w("views", 7.9, 0.3),
            w("Take", 13.0, 0.2), w("the", 13.2, 0.1), w("train", 13.3, 0.3),
        ];
        let segs = group_words(&words);
        assert_eq!(segs.len(), 3);
        assert_eq!(segs[0].text, "This is Lake Kawaguchi.");
        assert_eq!(segs[0].start, 3.0);
        assert_eq!(segs[1].text, "Come early for clearer views");
        assert_eq!(segs[2].start, 13.0);
    }

    #[test]
    fn reel_lines_keep_provenance_and_skip_repeats() {
        let transcript = vec![TranscriptSegment { start: 7.0, end: 9.0, text: "Come early for clearer Fuji views".into() }];
        let frames = vec![
            (5.0, vec!["Lake Kawaguchi".to_string(), "Follow".to_string()]),
            (9.0, vec!["Lake Kawaguchi".to_string(), "¥500 entry".to_string()]),
        ];
        let (lines, sources) = build_reel_lines(Some("Japan trip 🗻\nLake Kawaguchi"), &transcript, &frames);
        assert_eq!(lines[0], "audio 00:07: Come early for clearer Fuji views");
        assert_eq!(sources[0].kind, "audio");
        assert_eq!(sources[0].time_sec, Some(7.0));
        assert!(lines.contains(&"caption: Lake Kawaguchi".to_string()));
        // "Lake Kawaguchi" on screen was already in the caption; "Follow" is UI chrome.
        let on_screen: Vec<&String> = lines.iter().filter(|l| l.starts_with("on-screen")).collect();
        assert_eq!(on_screen, vec!["on-screen 00:09: ¥500 entry"]);
        assert_eq!(sources[lines.len() - 1].time_sec, Some(9.0));
    }
}

#[cfg(test)]
mod language_tests {
    use super::*;

    fn loc(id: &str, on_device: bool) -> SpeechLocale {
        SpeechLocale { id: id.into(), name: id.into(), on_device, engine: String::new() }
    }

    #[test]
    fn auto_uses_hints_then_english() {
        let supported = vec![loc("en-US", true), loc("ja-JP", true), loc("zh-CN", true), loc("ta-IN", true), loc("ko-KR", true)];
        let hints = vec![("ja".to_string(), 0.9), ("zh-Hans".to_string(), 0.05)];
        assert_eq!(candidate_locales("auto", &hints, &supported), vec!["ja-JP", "en-US"]);
        assert_eq!(candidate_locales("auto", &[("ta".into(), 0.9)], &supported), vec!["ta-IN", "en-US"]);
        // Real case: a romanised English caption was detected as Indonesian with modest confidence — ignore it.
        assert_eq!(candidate_locales("auto", &[("id".into(), 0.6)], &supported), vec!["en-US"]);
        assert_eq!(candidate_locales("auto", &[], &supported), vec!["en-US"]);
    }

    #[test]
    fn unsupported_languages_are_skipped_in_auto_but_kept_when_chosen() {
        let supported = vec![loc("en-US", true), loc("en-AU", true)];
        // Sinhala isn't supported by Apple speech on this Mac: auto falls back to English…
        assert_eq!(candidate_locales("auto", &[("si".into(), 0.95)], &supported), vec!["en-US"]);
        // …but an explicit choice is passed through so the user gets a clear "not supported" message.
        assert_eq!(candidate_locales("si-LK", &[], &supported), vec!["si-LK"]);
    }

    #[test]
    fn same_language_fallback_prefers_on_device() {
        let supported = vec![loc("zh-TW", false), loc("zh-HK", true)];
        assert_eq!(locale_for_language("zh-Hans", &supported).as_deref(), Some("zh-HK"));
    }
}
