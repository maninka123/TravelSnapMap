# Accuracy evaluation

`src-tauri/src/eval.rs` measures the deterministic parts of the pipeline on hand-labelled data in `src-tauri/eval/`
and fails the build if a quality gate is missed. Run it with:

```bash
cd src-tauri && cargo test --lib eval -- --nocapture
```

## What the data is — and isn't

The cases are **hand-written and hand-labelled**, modelled on the screenshot types and failure cases recorded in
the development log (same-name places in different countries, chain branches, suburbs, Maps omitting the country,
chats, receipts…). They are **not** a sample of a real library, and the AI extraction step (DeepSeek) is not
measured here because it needs the live service. Treat the numbers as regression checks for the code, not as the
accuracy users will see. Earlier real-data runs (100 screenshots, 15 Reels) are summarised in the development log.

## Results (measured 2026-10-09, release 0.5.0)

| Component | Cases | Result | Gate |
| --- | --- | --- | --- |
| On-device travel filter — travel posts kept for the AI (recall) | 30 travel | **30 / 30 (100%)** | ≥ 95% |
| On-device travel filter — non-travel screenshots skipped without any AI call | 25 non-travel | **24 / 25 (96%)** | — |
| On-device travel filter — of everything skipped, truly not travel (precision) | 24 skipped | **100%** | ≥ 95% |
| Place matching — correct candidate ranked first | 23 with a right answer | **23 / 23 (100%)** | ≥ 85% |
| Place matching — confirmed without asking | 23 | **19 / 23 (83%)**, all 19 correct | — |
| Place matching — wrong place confirmed without asking | 30 | **0 / 30 (0%)** | ≤ 5% |
| Place matching — cases with no right answer kept out of the map | 7 | **7 / 7** | — |
| Duplicates — merges that were correct (precision) | 7 merges | **7 / 7 (100%)** | ≥ 95% |
| Duplicates — real duplicates found (recall) | 8 | **7 / 8 (88%)** | ≥ 60% |

## Important caveat about the travel filter

The first measurement found the filter **dropping 5 of 30 travel posts (83% recall)** before they ever reached the
AI — a coastal walk, a “where to eat” list, a hiking route, a Chinese-language post and a city-walls entry fee. The
vocabulary was widened with general travel terms (e.g. “where to eat”, “walk”, “entry”, “bus”, 夜景, 地铁), which
brought this set to 30/30. Because the fix was made after looking at this same set, **100% is not an independent
estimate**: a held-out set (ideally real screenshots, labelled) is needed to measure it properly.

## Known misses

- `d11` — the entrance and the centre of a large park ~150 m apart under slightly different names are not merged
  automatically (it becomes a “same place twice?” question instead). Safe, but one extra question.
- Official vs common names (`m12`: Temple of the Tooth / Sri Dalada Maligawa) are ranked correctly but sent to review
  rather than confirmed, because the names don't look alike.

## Not measured yet

- DeepSeek extraction quality (places found, facts, false travel). The opt-in live tests in `src-tauri/src/tests.rs`
  (`live_*`, `#[ignore]`) run against a real key and library; they report counts but have no labelled ground truth.
- Apple Maps search quality itself (the evaluation uses fixed candidate lists).
