# TravelSnapMap — build progress

Ticked items are done and verified (tests pass / smoke-tested). Unticked items are in progress or planned.

## Milestone A — Tauri + React + Rust + Swift architecture
- [x] Port logic from the earlier SwiftUI draft (prompts, schemas, fact types, states, scoring, dedupe)
- [x] Swift `photos-bridge` helper (PhotoKit, Vision OCR, saliency, feature prints, MapKit search) — built with Command Line Tools only
- [x] Bridge smoke test: OCR with bounding boxes, MapKit search, saliency
- [x] Rust backend skeleton (Tauri 2) + JSON-lines bridge client
- [x] SQLite schema: places, screenshots, ocr_blocks, place_screenshots, travel_facts, place_images, trips, trip_places, processing_jobs, ai_cache (+ review_items, ai_usage_log, metrics, settings)
- [x] DeepSeek client (`deepseek-flash`, thinking disabled by default, JSON mode, retries, usage + cost tracking)
- [x] Local travel filter (Level 1) + AI routing (Level 2 / Level 3 escalation)
- [x] Place resolution scoring + MapKit provider (AI never supplies coordinates)
- [x] De-duplication, fact merging (contradictions + history kept), merge / split
- [x] Photo-region detection (OCR geometry, text density, saliency) + duplicate photo detection
- [x] Processing pipeline with persisted states, restart recovery, AI cache, offline parking
- [x] Processing queue: concurrency, pause / resume / cancel, progress events
- [x] Review actions (travel?, which place?, duplicate?, photo crop?, failures)
- [x] Rust tests: 42 passing (decoding, scoring, dedupe, crops, states, full pipeline with mocks)

## Milestone B — React UI
- [x] App shell, navigation, processing bar, onboarding
- [x] Map (MapLibre, clustering, filters, country chips, place preview)
- [x] Places library (grid/list, search, filters, sort)
- [x] Place detail (info cards with sources, photos, screenshots, nearby, trips, edit / merge / split)
- [x] Sources library (screenshots + Reels together) and screenshot + detail (OCR overlay, source highlight, actions, crop editor)
- [x] Review inbox
- [x] Trips
- [x] Settings (AI config, privacy, API key) + diagnostics
- [x] Frontend type-check + production build

## Milestone C — Instagram Reel import
- [x] Schema: reels, place_reels, fact source kind + timestamp
- [x] Swift bridge: audio extraction, on-device speech transcription (timestamped), keyframe sampling, Photos video import
- [x] Reel fetcher: URL → caption/creator metadata; media via yt-dlp when available
- [x] Reel pipeline: transcript → caption → keyframe OCR → one DeepSeek call → places → merge
- [x] Fallback: "Import Video from Photos" when Instagram media isn't accessible
- [x] UI: paste-URL import, Reels in the Sources library, Reel detail (video, saved voice audio, timestamped transcript, key snapshots)
- [x] Place info cards (Best Time, Getting There, Tickets & Cost, …) with a source on every fact
- [x] Tests for Reel pipeline (52 Rust tests passing in total)

## Milestone D — Ship
- [x] Run the app (`npm run tauri dev`) — launches cleanly, DB migrates, bridge starts
- [x] Live end-to-end check: real Vision OCR → DeepSeek → Apple Maps → SQLite ($0.0001 per screenshot)
- [ ] Scan your real Photos library (needs you to click "Allow" for Photos access)
- [x] README (setup, architecture, privacy, cost controls)
- [x] `.gitignore` (secrets, build output)
- [x] Push to GitHub (github.com/maninka123/TravelSnapMap)
