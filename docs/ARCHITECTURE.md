# Architecture

```text
React UI (src/)  ──invoke/events──▶  Rust core (src-tauri/src/)  ──JSON lines over stdio──▶  Swift helper (native/photos-bridge)
                                        │                                                     PhotoKit · Vision OCR · Speech
                                        ├── SQLite library (WAL, versioned migrations)          MapKit search · saliency
                                        ├── DeepSeek client (text only by default)
                                        └── yt-dlp (optional, Reel media)
```

## Frontend (`src/`)

| Folder | |
| --- | --- |
| `App.tsx` | Shell: sidebar (Explore, Import, My Trips, Settings), shortcuts (⌘1–3, ⌘,, ⌘I, ⌘F), detail sheet, toasts, first-run guide, window-wide file drop |
| `api/` | `services.ts` is the only module that calls the backend; `types.ts` mirrors the Rust records |
| `components/` | Shared UI: `ui.tsx` (Radix dialog/menu/popover, segmented control, toasts), `common.tsx`, pickers, editors |
| `features/explore` | `ExploreView` owns search/filters/scope/selection; `MapView` (map engine), `PlaceList` (virtualised), `Library` (grid), `model.ts` (pure, tested) |
| `features/import` | Drop zone, sources, live status, recent imports; hosts `review/` and `screenshots/` (Sources) tabs |
| `features/trips` | Trip list, itinerary with drag and drop, trip map; `itinerary.ts` (pure, tested) |
| `features/places`, `reels`, `screenshots` | Detail pages and editors |
| `features/settings`, `onboarding` | Settings sections; first-run guide |
| `styles/` | `tokens.css` (light/dark/forced theme), `base.css`, `layout.css`, `features.css` |
| `dev/` | Demo backend over Tauri's IPC mocks — `npm run demo` and the UI tests; compiled out of production builds |

## Backend (`src-tauri/src/`)

| Module | |
| --- | --- |
| `pipeline/` | Screenshot and Reel pipelines (OCR → local filter → AI → Maps → merge → photos), queue (one run at a time, pause/resume/cancel, resume after restart), user actions, merge/split/relocate |
| `services/ai` | `TravelAIService` trait (DeepSeek implementation), prompts, typed JSON schemas, on-device filter |
| `services/places` | Candidate ranking and auto-accept / review / reject decision |
| `db/` | Repositories, migrations v1–v7 (each in a transaction, with a backup copy first), `overrides_repo` |
| `backup.rs` | Backup zip, exports, trip Markdown, restore (validate → stage → swap at launch) |
| `eval.rs` | Accuracy gates on labelled data (tests only) |

## Corrections that survive re-processing

Re-processing a screenshot or Reel deletes and rebuilds only automatic results. User decisions are kept by:

- links and tips marked `is_user_verified` / `origin = user`,
- `source_decisions` — a name you removed from a source or answered “not a place” for is skipped next time,
- `place_aliases` — the Apple Maps id of a place you merged or relocated keeps pointing at the place you kept,
- answered review questions are kept (“Recently answered”).

## Data safety

- Every schema upgrade copies the library to `backups/pre-migration-v{old}-to-v{new}-…` and runs in a transaction.
- Imports are idempotent: PhotoKit ids and file paths are unique; dropped files are stored by content hash; AI
  results are cached by content and prompt version.
- Restore never deletes: the current library is moved to `backups/before-restore-…`, and is put back if the swap fails.
- Secrets: the DeepSeek key is in the Keychain; backups strip secret-like settings; logs never contain screenshot text.
- Links the UI opens are limited to http(s) and the Photos privacy pane.
