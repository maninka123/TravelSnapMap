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
- [x] Rust tests (now 74 + 3 opt-in live tests)

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
- [x] Tests for Reel pipeline

## Milestone D — Ship
- [x] Run the app (`npm run tauri dev`) — launches cleanly, DB migrates, bridge starts
- [x] Live end-to-end check: real Vision OCR → DeepSeek → Apple Maps → SQLite ($0.0001 per screenshot)
- [x] README (setup, architecture, privacy, cost controls)
- [x] `.gitignore` (secrets, build output)
- [x] Push to GitHub (github.com/maninka123/TravelSnapMap)

## Milestone E — Reliability on real data
- [x] Test mode: process 10 / 25 / 50 / 100 real screenshots (random or newest) with a results report
- [x] "Scan New Screenshots": only PhotoKit IDs / file paths not already in the database
- [x] Setting: automatically process new screenshots (default OFF)
- [x] Import Folder: screenshots from any device (PNG/JPG/HEIC folders), same pipeline and "only new" logic
- [x] Reel stages shown clearly (Caption → Video → Audio → Transcript → Keyframes → OCR → AI → Places); later stages continue if one fails
- [x] Reel transcription: Auto language + manual re-transcribe (English, Japanese, Chinese, Korean, Sinhala, Tamil, …)
- [x] Place cards polish (Warnings card, per-card sources, every fact keeps its source/timestamp)
- [x] Security: DeepSeek key in macOS Keychain (migrated from SQLite); production CSP (dev keeps a relaxed one for hot reload)
- [x] Fix: macOS killed the helper on permission requests in dev (TCC "responsible process") — helper now requests permissions itself
- [x] UI restyled with the Apple design framework (apple-design skill installed permanently)
- [x] Real test: 100 real screenshots (random sample) — 0 failures, 81% place auto-resolution, est. $0.018–0.037 per 100 at official rates
- [x] Real test: 15 real Instagram Reels — 0 failures, 14/15 usable (1 login-only → "import from Photos"), 42 places (39 verified), est. $0.005–0.010 total
- [x] Fix issues found in real testing (each one has a regression test):
  - [x] Local filter missed real travel posts (e.g. Wangxian Valley TikTok, restaurant with street address) → recall improved
  - [x] Countries/continents extracted as "places" → dropped
  - [x] Apple Maps biased to your own location → searches centred on the place's city/region/country, alt-language names tried
  - [x] Exact match rejected when Maps omitted the country; same-name place in another city accepted → fixed scoring
  - [x] Model added text after JSON; long Reels cut off at the output limit → tolerant parsing + automatic larger retry
  - [x] Review noise: "is this travel?" with nothing to map → low-confidence instead; near-identical screenshots share one AI result
  - [x] Choosing a place in Review lost Reel timestamps / OCR provenance → facts are moved, not re-created
  - [x] Re-processing added to old AI cost/latency → reset per run
  - [x] Apple Maps throttling → 1.3 s spacing + backoff; auto-retry only parked items (never failed ones)
  - [x] Instagram rate-limits → yt-dlp retries; DeepSeek hiccups retried, "offline" only when truly offline
  - [x] Auto language picked wrong languages → only strong caption hints reorder candidates
  - [x] Song lyrics used as "speech" → low-confidence transcripts kept but excluded from AI input
  - [x] Thinking escalation on Reels (+23 s, ~6x cost, same result) → disabled for Reels
- [x] Speed: one yt-dlp call, audio and keyframes in parallel, concurrent OCR, 2-pass keyframes (Reel ~42 s → ~20 s)
- [x] Output tokens −33 % (short reason, no redundant quotes); cost ≈ $0.011 per 100 screenshots
- [x] Production: error boundaries, log file, Keychain, strict CSP, per-stage timings

## Milestone F — Safe for long-term use (v0.2.1)
- [x] Estimated AI cost: official deepseek-flash peak/off-peak pricing, configurable, estimates recalculated from stored tokens
- [x] GitHub Actions CI (npm ci, typecheck, build, cargo test) — no Photos/DeepSeek/MapKit needed
- [x] Backup TravelSnapMap (.zip, never the API key); Export Places as JSON / GeoJSON
- [x] Pre-migration backups + transactional migrations with a clear error on failure
- [x] Minimum macOS lowered to 15 with fallbacks for the two macOS-26-only APIs
- [x] Docs updated (pricing, benchmarks, test count)
- [x] Double-clicked app uses the project's .env.local key; failed-for-temporary-reasons screenshots re-queued (no data deleted)
- [x] iCloud Photos download errors retried with backoff, then parked for automatic retry
- [x] Bulk Reel import: paste many links or choose a text file; processed one after another
- [x] `npm run tauri …` works even when Terminal doesn't have Rust on its PATH

- [x] Test run moved from Sources to Settings → Diagnostics ("Test on a sample")

## Milestone G — Find, plan and remember (v0.3)
- [x] Map search with type-ahead suggestions (places, cities, countries); Smart Search tried and removed at your request
- [x] ＋ Add Place from Apple Maps (no screenshot needed; duplicates open the existing place)
- [x] Freshness & conflict warnings: stale prices/hours, sources that disagree, different best-time advice — on places, Places badges and trips
- [x] My visit: visit date, your own notes, your own photos from Photos (near the place or by date), your photo as cover
- [x] Library schema v4 (visits + memories) with pre-migration backup; backups include your attached photos
- [x] Settings: cost summary on top, standard prices automatic (custom under Advanced), grouped colour-coded diagnostics
- [x] Map preview card redesigned (photo with title, category, status, Apple Maps button)
- [x] Map redesign: quiet basemap (Positron/Dark, minor labels hidden), Filters popover, “In view” panel grouped by country → city, sized clusters with ring + hover, category pins with selected state, Fit-all control; country chip row removed
- [x] Map style picker (Light · Bright · Colourful · Dark · Blue · Match system), remembered; Light by default
- [x] Apple-design pass on the newer UI: Reduce Motion/Transparency for all popovers, instant press feedback on rows, no fake press on non-buttons
- [x] Consistency pass: map/sidebar counts explained, Places filters show only what exists, clearer photo sections, wider photo search for cities, map card keeps in sync
- [x] Small extras: change status from the map card, Open in Apple Maps on place pages, ⌘1–6 / ⌘F shortcuts
- [x] README rewritten (short, professional)
- [x] Edit processed results: edit/delete/add tips on screenshots, Reels and places (kept on reprocess); add/remove places on Reels; remove a wrong place from a screenshot
- [x] Place picker everywhere: your saved places first ("Saved"), then Apple Maps as you type
- [x] Review: All / Screenshots / Reels switch
- [x] Map: 9 place groups (Food & drink, Stay, Sights, Nature, Shopping, Activities, Transport, Cities & regions, Other) with one icon set on pins, filters, badges; picking a city zooms past clustering
