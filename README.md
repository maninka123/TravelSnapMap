# TravelSnapMap

Turn years of scattered travel screenshots — from Apple Photos or any folder — and Instagram Reels you paste in, into an organised,
searchable map of places, photos and tips, while keeping every original screenshot and Reel as the source.

**macOS · local-first · Tauri + React + TypeScript + Rust + SQLite + a small Swift helper (PhotoKit, Vision, Speech, MapKit) · DeepSeek (`deepseek-flash`)**

```
Apple Photos / screenshot folder ─► Swift bridge ─► Apple Vision OCR (on your Mac)
                                                │
Instagram Reel URL ─► video ─► audio transcript (on-device) + caption + key-frame OCR
                                                │
                                   Local travel filter (Level 1) ── clearly unrelated → stop, no AI call
                                                │
                                   DeepSeek Flash, thinking off (Level 2) — one structured JSON call
                                                │   (Level 3: one thinking retry, only if ambiguous)
                                   Place resolution via Apple Maps (AI never supplies coordinates)
                                                │
                                   De-duplication + merge (contradictions and history kept)
                                                │
                                             SQLite
                                                │
                     React UI: Map · Places · Sources · Review · Trips · Settings
```

The **Place** is the central object. Screenshots and Reels are evidence attached to it; every fact keeps its source
(screenshot + OCR block, or Reel + audio/caption/on-screen timestamp).

---

## Quick start

**Easiest:** double-click **`Open TravelSnapMap.command`** in the project folder. It builds the app the first time (or whenever the code changed) and opens it.

Requirements (macOS 15 Sequoia or later; macOS 26+ recommended):

| Tool | Install |
|---|---|
| Xcode **Command Line Tools** (the full Xcode app is not needed) | `xcode-select --install` |
| Node.js 20+ | `brew install node` |
| Rust | `curl https://sh.rustup.rs -sSf \| sh` |
| *Optional:* yt-dlp (downloads Reel videos from a pasted link) | `brew install yt-dlp`, or the standalone binary: `curl -L -o ~/.local/bin/yt-dlp https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp_macos && chmod +x ~/.local/bin/yt-dlp` |

```bash
cp .env.example .env.local        # then put your DeepSeek key in it
npm install
npm run tauri dev
```

UI changes hot-reload. Rust and Swift changes rebuild automatically on the next `tauri dev` restart
(`src-tauri/build.rs` compiles the Swift helper with `swiftc`; `src-tauri/.cargo/config.toml` points builds at the
Command Line Tools, so an un-accepted Xcode licence never blocks you).

### Permissions

On first use macOS asks for **Photos** access as **“TravelSnapMap Photos Bridge”** (the small helper that reads
screenshots). The helper makes itself responsible for its own privacy requests, so this works the same in `tauri dev`
(launched from Terminal, VS Code or Cursor) and in the bundled app. Reel transcription uses Apple's on-device
`SpeechTranscriber` and needs no extra permission; only languages it doesn't cover fall back to the classic recogniser,
which asks for Speech Recognition access.

### DeepSeek key

The key is looked up in this order, and is never committed:

1. `DEEPSEEK_API_KEY` environment variable / `.env.local` (development)
2. A key pasted in **Settings**, stored in the **macOS Keychain** (never in the database or a file)

For production, set **Settings → API base URL** to your own backend proxy so the secret stays on a server.

---

## Using it

- **📸 Scan New Screenshots** (Sources, or the Map's first-run card) — compares PhotoKit asset IDs (and file paths of
  your folders) with the database and processes only screenshots it hasn't seen. Interrupted work resumes; failed items
  are retried only when you ask (**Retry failed**), so a persistent error never burns AI calls in a loop.
- **🧪 Test run…** — process 10 / 25 / 50 / 100 unprocessed screenshots (random sample or newest) and get a report:
  travel / not travel / needs review / failed, places found, place-resolution success, OCR and AI time, and the
  estimated DeepSeek cost. Past runs are listed in Settings.
- **📁 Import Folder…** — screenshots from any device (PNG, JPG, HEIC…). Same pipeline, same "only new" logic.
- **Automatically process new screenshots** (Settings, **off by default**) — when on, new screenshots are processed as
  soon as Photos reports them.
- **🎬 Import Reels** — paste one link, a whole list (any text with links in it), or choose a text file (.txt, .md, .csv…) containing links. Duplicates are skipped; several Reels are processed one after another. The Reel page shows each stage (Caption → Video → Audio →
  Transcript → Keyframes → OCR → AI extraction → Places resolved) with timings; a failed stage never stops the others.
  **Auto language** picks the spoken language from the caption / on-screen text; you can re-transcribe in English,
  Japanese, Chinese, Korean, Tamil and every other language Apple supports on your Mac (Sinhala isn't supported by
  Apple speech recognition — its caption and on-screen text are still used).

## Real-data results

Measured on this Mac with the real pipeline (Vision OCR → local filter → DeepSeek Flash → Apple Maps):

| | 100 random real screenshots | 15 real Instagram Reels |
|---|---|---|
| Failures | 0 | 0 (14 of 15 usable; 1 login-only post asks for the video from Photos) |
| Handled locally, no AI call | 42 % | — |
| Places auto-resolved | 81 % | 39 of 42 places (93 %) |
| Average time | 0.9 s OCR · 2.6 s AI · 3.7 s total | 8–37 s per Reel (75 s for a 19-place itinerary) |
| Estimated AI cost (official deepseek-flash rates) | **$0.018 off-peak – $0.037 peak per 100 screenshots** | **$0.0003–$0.0007 per Reel** |

Each issue found in these runs became a regression test (`cargo test`), e.g. a travel TikTok the local filter missed,
a place Maps returned without a country, a Kyoto spa matched to a Gion in Hiroshima, and Reels with many places that
exceeded the output limit.

## How it works

### Screenshots
1. **Discover** — PhotoKit's `photoScreenshot` subtype only; the rest of the library is never read. New screenshots
   are picked up automatically (Photos change observer) and on each scan.
2. **OCR** — Apple Vision `RecognizeTextRequest` (accurate, automatic language detection, language correction).
   Every block keeps text, confidence and a normalised bounding box (`ocr_blocks` table).
3. **Level 1 — local filter** — keywords, country names, source-app signatures, negative signals (code, chats,
   banking…). Clearly unrelated screenshots stop here with **no AI request**.
4. **Level 2 — DeepSeek Flash** (thinking explicitly disabled) — one request returns classification *and*
   extraction as strict JSON (places, city/region/country, category, typed facts with source line numbers, creator,
   source app). Validated locally against typed structs; invalid JSON is retried once, then recorded as a failure
   without touching existing records.
5. **Level 3** — only when the result is genuinely ambiguous: a single retry with thinking enabled.
6. **Thresholds** — travel probability ≥ 0.80 → processed; 0.50–0.79 → Review Inbox; < 0.50 → not travel
   (overridable). All thresholds are editable in Settings.
7. **Place resolution** — Apple Maps (`MKLocalSearch`) candidates scored on name similarity (incl. alternative
   names), city, country, category compatibility and proximity to other places in the same screenshot. High → verified
   pin; medium → provisional place + Review; low → Review with candidates, no pin.
8. **Merge** — same MapKit identifier, or very close + similar name ⇒ same place. Near-but-uncertain ⇒ a
   "Same place?" review. New facts are added; duplicates skipped; contradictions and changed prices are kept side by side
   with their screenshot dates.
9. **Photos** — local region detection (OCR geometry, text density, colour/detail statistics, Vision saliency);
   map apps and UI are never used as photos. Uncertain crops go to AI vision only if allowed, else to Review.
   Near-duplicate photos are collapsed with Vision feature prints, keeping provenance of every source.

### Instagram Reels
Paste a Reel/Post link (**Sources → Import Reel**):
1. Metadata (caption, creator, date) and the video via **yt-dlp** if installed (optionally with your browser's
   cookies), else the page's public preview tags.
2. **Audio first** — the voice track is saved (`.m4a`) and transcribed **on-device** with word timestamps, grouped
   into lines like `00:07 Come early in the morning for clearer Fuji views.`
3. **Caption**, then **key-frame OCR** — only visually distinct frames (default 8), never every frame.
4. **One DeepSeek call** on the text evidence. An image (one compressed cover frame) is sent only when the text is
   too thin. The video is never uploaded.
5. Places are resolved independently and merged into existing places — a Reel and a screenshot about the same place
   end up on one Place page. Facts show *Instagram Reel · @creator · Audio · 00:07* and jump to that moment.
6. If Instagram doesn't provide the video: the URL and caption are kept, and **Import video from Photos** is offered.

### Cost controls
- Local OCR, local filtering, local photo analysis, local transcription; AI only when it adds value.
- One combined request per screenshot/Reel, compact stable system prompt (benefits from DeepSeek prompt caching),
  capped output tokens, no reasoning text.
- `ai_cache` keyed by OCR-content hash + model + prompt version: identical content is never sent twice.
- Versioned prompts (`AI_PROMPT_VERSION`) with selective re-processing (failed / needs review / older prompt / all).
- Daily AI request limit, pause/resume, year range.
- Diagnostics: requests (text / vision / thinking), tokens, cache hits, requests avoided locally, estimated cost,
  cost per travel screenshot and per place, per-stage timings.

A real run on a sample screenshot ("Lake Kawaguchi… Best view of Mount Fuji around sunrise. Take the train from
Shinjuku.") used ~584 input + 208 output tokens ≈ **$0.0002–0.0004 estimated AI cost**.

**Pricing assumptions** (Settings → Estimated AI cost, editable): deepseek-flash per 1M tokens — peak $0.30 input (cache miss) / $0.006 (cache hit) / $1.20 output; off-peak half. Peak = Mon–Fri 01:00–04:00 and 06:00–10:00 UTC (Chinese public holidays not modelled). Token counts are stored exactly; estimates are recalculated when pricing changes.

### Privacy
- OCR, speech transcription, photo detection and map matching run on your Mac.
- DeepSeek receives only recognised text, the date and small hints (app name, visible @handle). Images are sent only
  for near-textless screenshots or uncertain crops, and never if **Settings → Never send screenshot images to AI** is on.
- Logs never contain screenshot contents.

---

## Project layout

```
native/photos-bridge/     Swift helper: PhotoKit, Vision (OCR, saliency, feature prints), AVFoundation + Speech, MapKit
src-tauri/src/
  services/native.rs      PhotoLibraryService / OcrService / VisionService / MediaService + JSON-lines bridge client
  services/ai/            TravelAIService trait, DeepSeek client, prompts, strict JSON types, local filter
  services/places/        PlaceService + candidate scoring (MapKit provider via the bridge)
  services/reels.rs       Instagram metadata + media (yt-dlp / Open Graph)
  pipeline/               screenshot + Reel pipelines, merge/dedupe, photo regions, queue, user actions
  db/                     SQLite schema + migrations, repository
  commands.rs             Tauri commands (the UI's only API)
src/                      React + TypeScript UI (api/ services, features/ screens)
TODO.md                   Build progress checklist
```

## Tests

```bash
cd src-tauri && cargo test          # 74 tests (+3 opt-in live tests): pipelines with mocks, real-data regressions, migrations, backup, pricing
# Live checks (use your key; tiny cost):
TSM_LIVE_IMAGE=/path/to/screenshot.jpg cargo test live_screenshot -- --ignored --nocapture
TSM_LIVE_COUNT=100 TSM_LIVE_DB=/tmp/v.sqlite cargo test live_photos -- --ignored --nocapture   # real Photos sample
TSM_LIVE_REELS=urls.txt TSM_LIVE_DB=/tmp/r.sqlite cargo test live_reels -- --ignored --nocapture   # real Reels
npm run build                       # type-check + production frontend build
```

## Backup, export and safety

- **Settings → Backup TravelSnapMap**: a .zip with the database snapshot, place photos/crops, thumbnails, Reel audio/key snapshots and config.json. Never the API key (Keychain only). Restore steps are in the zip's README.txt.
- **Export Places as JSON / GeoJSON** (GeoJSON = verified places with name, city, country, category, status, notes, source count).
- Before any schema migration the library is copied to `backups/pre-migration-*.sqlite`; each step is one transaction, so a failed upgrade leaves the original intact and shows a clear message.

## macOS version

Minimum **macOS 15**. Only two features use macOS 26 APIs, with fallbacks: Apple Maps address details (`MKMapItem.location/address/addressRepresentations` → `placemark` on 15) and on-device Reel transcription (`SpeechTranscriber`/`SpeechAnalyzer` → `SFSpeechRecognizer` on 15, fewer on-device languages). Vision OCR, Photos and MapKit search need only macOS 15. Building requires the macOS 26 SDK. CI: GitHub Actions (`.github/workflows/ci.yml`).

## Notes & limits
- `ocr_blocks` are SQLite rows; OCR for the whole library stays local.
- Logs (release builds): `~/Library/Logs/TravelSnapMap/travelsnapmap.log` — screenshot contents are never logged.
- Production CSP allows only the app itself, local files and the map tile host; development keeps a relaxed policy for hot reload.
- Instagram frequently blocks anonymous downloads. With yt-dlp + browser cookies most public Reels work; otherwise use
  the Photos fallback.
- Map tiles come from OpenFreeMap (no key). Place search uses Apple Maps through the bridge.
- iOS and iCloud sync are intentionally out of scope for this milestone (macOS, local-first).
