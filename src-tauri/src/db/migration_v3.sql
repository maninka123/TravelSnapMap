-- v3: measurement for real-data validation, Reel stage tracking, transcription language.

ALTER TABLE screenshots ADD COLUMN ocr_ms INTEGER NOT NULL DEFAULT 0;
ALTER TABLE screenshots ADD COLUMN pipeline_ms INTEGER NOT NULL DEFAULT 0;
ALTER TABLE screenshots ADD COLUMN places_extracted INTEGER NOT NULL DEFAULT 0;
ALTER TABLE screenshots ADD COLUMN places_auto_resolved INTEGER NOT NULL DEFAULT 0;

-- Per-stage progress: {"caption": {"status": "done", "detail": "…"}, …}
ALTER TABLE reels ADD COLUMN stages TEXT NOT NULL DEFAULT '{}';
-- 'auto' or a locale such as ja-JP chosen by the user for re-transcription.
ALTER TABLE reels ADD COLUMN transcript_locale_override TEXT;
ALTER TABLE reels ADD COLUMN transcript_confidence REAL;
ALTER TABLE reels ADD COLUMN places_extracted INTEGER NOT NULL DEFAULT 0;
ALTER TABLE reels ADD COLUMN places_auto_resolved INTEGER NOT NULL DEFAULT 0;

-- Validation / scan runs, so results can be reviewed as a report.
CREATE TABLE IF NOT EXISTS test_runs (
    id              TEXT PRIMARY KEY,
    kind            TEXT NOT NULL,                    -- validation | scanNew
    requested       INTEGER NOT NULL DEFAULT 0,
    sample          TEXT NOT NULL DEFAULT 'newest',   -- newest | random
    screenshot_ids  TEXT NOT NULL DEFAULT '[]',
    started_at      TEXT NOT NULL,
    finished_at     TEXT
);
