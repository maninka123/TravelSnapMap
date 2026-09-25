-- v2: Instagram Reels as a second kind of evidence, alongside screenshots.

CREATE TABLE IF NOT EXISTS reels (
    id                  TEXT PRIMARY KEY,
    url                 TEXT NOT NULL,                -- original Instagram URL (the source)
    shortcode           TEXT,
    creator             TEXT,
    caption             TEXT,
    posted_at           TEXT,
    media_path          TEXT,                         -- local video copy
    audio_path          TEXT,                         -- saved voice audio (.m4a)
    thumbnail_path      TEXT,
    duration_sec        REAL,
    media_source        TEXT,                         -- ytdlp | og | photos
    status              TEXT NOT NULL DEFAULT 'discovered',
    status_detail       TEXT,
    failure_count       INTEGER NOT NULL DEFAULT 0,
    classification      TEXT NOT NULL DEFAULT 'unknown',
    travel_confidence   REAL NOT NULL DEFAULT 0,
    transcript          TEXT NOT NULL DEFAULT '[]',   -- JSON [{start, end, text}]
    transcript_locale   TEXT,
    content_hash        TEXT,
    ai_model            TEXT,
    ai_prompt_version   INTEGER NOT NULL DEFAULT 0,
    ai_input_tokens     INTEGER NOT NULL DEFAULT 0,
    ai_output_tokens    INTEGER NOT NULL DEFAULT 0,
    ai_cost             REAL NOT NULL DEFAULT 0,
    created_at          TEXT NOT NULL,
    processed_at        TEXT
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_reels_shortcode ON reels(shortcode) WHERE shortcode IS NOT NULL;

CREATE TABLE IF NOT EXISTS reel_keyframes (
    id              TEXT PRIMARY KEY,
    reel_id         TEXT NOT NULL REFERENCES reels(id) ON DELETE CASCADE,
    time_sec        REAL NOT NULL,
    image_path      TEXT NOT NULL,
    ocr_text        TEXT NOT NULL DEFAULT '',
    ocr_blocks      TEXT NOT NULL DEFAULT '[]',       -- JSON [{text, confidence, x, y, width, height}]
    is_cover        INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS idx_keyframes_reel ON reel_keyframes(reel_id, time_sec);

CREATE TABLE IF NOT EXISTS place_reels (
    place_id        TEXT NOT NULL REFERENCES places(id) ON DELETE CASCADE,
    reel_id         TEXT NOT NULL REFERENCES reels(id) ON DELETE CASCADE,
    extracted_name  TEXT NOT NULL DEFAULT '',
    confidence      REAL NOT NULL DEFAULT 0,
    origin          TEXT NOT NULL DEFAULT 'ai',
    is_user_verified INTEGER NOT NULL DEFAULT 0,
    created_at      TEXT NOT NULL,
    PRIMARY KEY (place_id, reel_id)
);
CREATE INDEX IF NOT EXISTS idx_place_reels_r ON place_reels(reel_id);

-- Facts, photos and reviews can now point at a Reel, with the exact moment they came from.
ALTER TABLE travel_facts ADD COLUMN reel_id TEXT REFERENCES reels(id) ON DELETE SET NULL;
ALTER TABLE travel_facts ADD COLUMN source_kind TEXT NOT NULL DEFAULT 'screenshot';  -- screenshot | audio | caption | keyframe | user
ALTER TABLE travel_facts ADD COLUMN source_time_sec REAL;
ALTER TABLE place_images ADD COLUMN reel_id TEXT REFERENCES reels(id) ON DELETE SET NULL;
ALTER TABLE review_items ADD COLUMN reel_id TEXT REFERENCES reels(id) ON DELETE CASCADE;
