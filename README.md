# TravelSnapMap

TravelSnapMap is a local-first macOS app that turns scattered travel screenshots and Instagram Reels into an organised personal map of places, tips, photos and travel memories.

It scans screenshots from Apple Photos, understands travel information using Apple Vision and AI, resolves real places with Apple Maps, and combines multiple screenshots and Reels about the same place. Every extracted detail stays linked to its original source.

![TravelSnapMap — saved places on the map](docs/screenshot.png)

Built with Tauri 2, React + TypeScript, Rust + SQLite and a small Swift helper (PhotoKit, Vision, Speech, MapKit).
macOS 15+, local-first.

## Features

| Area | What it does |
| --- | --- |
| **Screenshots → places** | Scans Apple Photos (or any folder) for new screenshots, reads them on your Mac with Apple Vision and finds the places, tips, prices and opening hours. Clearly unrelated screenshots never leave your Mac. Screenshots already done are always skipped. |
| **Instagram Reels** | Paste one link, a list or a text file of links. Audio is transcribed on-device, key frames are read, and each place is saved with the moment it's mentioned. |
| **Map** | Country flag bubbles → click a country for its city bubbles → click a city for its pins. Suburbs and districts are grouped under their real city (Surry Hills → Sydney, Pudong → Shanghai). Well-supported cities get their own label; a city's own pin stands out. Category icons, filters, type-ahead search, an "In view" list and five map styles. |
| **Places** | Grouped by country, with places in the same city next to each other. Each place page shows every tip with its source, keeps conflicting advice side by side, and warns about out-of-date or conflicting info (**Before you go**). |
| **Photos & sources** | One grid per place: its screenshots and Reels plus your own photos. Add photos by drag & drop, paste (⌘V) or file picker (HEIC works); add details to any photo; set any card as the cover and drag the cover to choose what shows. |
| **Editing** | Edit, delete or add tips; change or remove a place on a screenshot or Reel; correct a location (it merges with a place you already have there); mark a source **Not travel** or **Ignore** it. Your edits survive reprocessing. |
| **Review** | Everything uncertain in one inbox, with **Recently reviewed** to see and change your answers. The same questions appear inside each screenshot or Reel viewer. |
| **Trips & memories** | Group places into trips and days. After a visit, add the date, your notes and your own photos from Photos (found by location or date). |
| **Add Place** | Save a place you know without a screenshot: saved places are suggested first, then Apple Maps. |
| **Library health** | Pins with no screenshot or Reel behind them (and nothing you added) are removed automatically. Duplicate places are detected, and every schema upgrade makes a backup first. |
| **Backup & export** | One .zip with the library, photos and settings (never the API key), or export places as JSON / GeoJSON. |

### Next up

| Planned | Why |
| --- | --- |
| Signed & notarized download (Developer ID) | Open on any Mac without security warnings |
| First-run DeepSeek key setup | Other people don't have a `.env.local` |
| Tidy place kinds automatically | Some places named after a city are saved as "station" or "activity" |

## Privacy and cost

- Text recognition, speech transcription, image analysis and place matching all run on your Mac.
- DeepSeek receives only the recognised text of likely-travel screenshots and Reels — never images unless you allow it.
- Coordinates always come from Apple Maps, never from the AI.
- Measured cost: about **$0.02–0.04 per 100 screenshots** (deepseek-flash, peak/off-peak pricing shown in Settings).
- Your API key stays in `.env.local` or the macOS Keychain. It is never committed or included in backups.

## Getting started

**Requirements:** macOS 15 or later, Xcode Command Line Tools (`xcode-select --install`), Node.js 20+, Rust
([rustup](https://rustup.rs)). Optional: [yt-dlp](https://github.com/yt-dlp/yt-dlp) for importing Reels by link.

```bash
git clone https://github.com/maninka123/TravelSnapMap.git
cd TravelSnapMap
cp .env.example .env.local     # add your DEEPSEEK_API_KEY
npm install
npm run tauri dev              # or: npm run tauri build
```

Or double-click **`Open TravelSnapMap.command`**: it builds the app when needed and opens it.

On first use macOS asks for Photos access for **TravelSnapMap Photos Bridge** — the small helper that reads your
screenshots.

## Keyboard shortcuts

| Shortcut | Action |
| --- | --- |
| ⌘1 – ⌘6 | Map, Places, Sources, Review, Trips, Settings |
| ⌘F | Search on the current page |
| ↑ ↓ ↩ | Move through and pick search suggestions |

## Development

```bash
npm run build                       # type-check + frontend build
cd src-tauri && cargo test          # 82 Rust tests: pipelines, migrations, backup, pricing, warnings, library rules
```

```text
native/photos-bridge/   Swift helper: PhotoKit, Vision, Speech, MapKit (JSON over stdio)
src-tauri/src/          Rust: pipelines, DeepSeek client, place resolution, SQLite, Tauri commands
src/                    React UI
```

CI runs type-check, build and tests on every push (`.github/workflows/ci.yml`).
