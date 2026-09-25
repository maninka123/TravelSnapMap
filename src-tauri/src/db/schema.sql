-- TravelSnapMap schema v1. Place is the central entity; screenshots are evidence attached to it.

CREATE TABLE IF NOT EXISTS places (
    id                  TEXT PRIMARY KEY,
    canonical_name      TEXT NOT NULL,
    alternative_names   TEXT NOT NULL DEFAULT '[]',   -- JSON array
    map_identifier      TEXT,                         -- MapKit place ID: strongest dedupe signal
    latitude            REAL NOT NULL,                -- always from the map provider or the user, never AI
    longitude           REAL NOT NULL,
    address             TEXT,
    city                TEXT,
    region              TEXT,
    country             TEXT,
    country_code        TEXT,
    category            TEXT NOT NULL DEFAULT 'other',
    verification        TEXT NOT NULL DEFAULT 'verified',
    origin              TEXT NOT NULL DEFAULT 'mapKit',
    is_user_verified    INTEGER NOT NULL DEFAULT 0,   -- user data is never overwritten by automation
    personal_status     TEXT NOT NULL DEFAULT 'wantToVisit',
    notes               TEXT NOT NULL DEFAULT '',
    summary_text        TEXT,
    summary_fact_ids    TEXT NOT NULL DEFAULT '[]',
    summary_generated_at TEXT,
    hero_image_id       TEXT,
    created_at          TEXT NOT NULL,
    updated_at          TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_places_map_id ON places(map_identifier);
CREATE INDEX IF NOT EXISTS idx_places_coords ON places(latitude, longitude);

CREATE TABLE IF NOT EXISTS screenshots (
    id                  TEXT PRIMARY KEY,
    photos_id           TEXT NOT NULL UNIQUE,         -- PHAsset.localIdentifier
    creation_date       TEXT,
    width               INTEGER NOT NULL DEFAULT 0,
    height              INTEGER NOT NULL DEFAULT 0,
    discovered_at       TEXT NOT NULL,
    processed_at        TEXT,
    processing_version  INTEGER NOT NULL DEFAULT 0,
    ocr_version         INTEGER NOT NULL DEFAULT 0,
    ai_prompt_version   INTEGER NOT NULL DEFAULT 0,
    status              TEXT NOT NULL DEFAULT 'discovered',
    status_detail       TEXT,
    failure_count       INTEGER NOT NULL DEFAULT 0,
    classification      TEXT NOT NULL DEFAULT 'unknown',
    user_classification TEXT,                         -- overrides every automatic decision
    travel_confidence   REAL NOT NULL DEFAULT 0,
    local_travel_score  REAL NOT NULL DEFAULT 0,
    source_type         TEXT NOT NULL DEFAULT 'unknown',
    creator             TEXT,
    ocr_full_text       TEXT NOT NULL DEFAULT '',
    ocr_hash            TEXT,
    image_path          TEXT,                         -- local cached export (regenerable from Photos)
    thumbnail_path      TEXT,
    escalation_level    INTEGER NOT NULL DEFAULT 0,   -- 1 local, 2 AI, 3 AI + thinking
    ai_model            TEXT,
    ai_thinking         INTEGER NOT NULL DEFAULT 0,
    ai_image_used       INTEGER NOT NULL DEFAULT 0,
    ai_input_tokens     INTEGER NOT NULL DEFAULT 0,
    ai_output_tokens    INTEGER NOT NULL DEFAULT 0,
    ai_latency_ms       INTEGER NOT NULL DEFAULT 0,
    ai_retry_count      INTEGER NOT NULL DEFAULT 0,
    ai_cost             REAL NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS idx_screenshots_status ON screenshots(status);
CREATE INDEX IF NOT EXISTS idx_screenshots_date ON screenshots(creation_date);

CREATE TABLE IF NOT EXISTS ocr_blocks (
    id              TEXT PRIMARY KEY,
    screenshot_id   TEXT NOT NULL REFERENCES screenshots(id) ON DELETE CASCADE,
    idx             INTEGER NOT NULL,                 -- reading order
    text            TEXT NOT NULL,
    confidence      REAL NOT NULL,
    x REAL NOT NULL, y REAL NOT NULL, width REAL NOT NULL, height REAL NOT NULL,  -- normalised, top-left
    language        TEXT
);
CREATE INDEX IF NOT EXISTS idx_ocr_blocks_screenshot ON ocr_blocks(screenshot_id, idx);

CREATE TABLE IF NOT EXISTS place_screenshots (
    place_id        TEXT NOT NULL REFERENCES places(id) ON DELETE CASCADE,
    screenshot_id   TEXT NOT NULL REFERENCES screenshots(id) ON DELETE CASCADE,
    extracted_name  TEXT NOT NULL DEFAULT '',
    confidence      REAL NOT NULL DEFAULT 0,
    origin          TEXT NOT NULL DEFAULT 'ai',
    is_user_verified INTEGER NOT NULL DEFAULT 0,
    created_at      TEXT NOT NULL,
    PRIMARY KEY (place_id, screenshot_id)
);
CREATE INDEX IF NOT EXISTS idx_place_screenshots_s ON place_screenshots(screenshot_id);

CREATE TABLE IF NOT EXISTS travel_facts (
    id              TEXT PRIMARY KEY,
    place_id        TEXT NOT NULL REFERENCES places(id) ON DELETE CASCADE,
    screenshot_id   TEXT REFERENCES screenshots(id) ON DELETE SET NULL,
    type            TEXT NOT NULL,
    text            TEXT NOT NULL,
    source_quote    TEXT,
    confidence      REAL NOT NULL DEFAULT 0,
    source_block_ids TEXT NOT NULL DEFAULT '[]',
    valid_from      TEXT,                             -- screenshot date: when this was believed true
    origin          TEXT NOT NULL DEFAULT 'ai',
    created_at      TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_facts_place ON travel_facts(place_id);

CREATE TABLE IF NOT EXISTS place_images (
    id              TEXT PRIMARY KEY,
    place_id        TEXT NOT NULL REFERENCES places(id) ON DELETE CASCADE,
    screenshot_id   TEXT REFERENCES screenshots(id) ON DELETE SET NULL,
    crop_x REAL NOT NULL, crop_y REAL NOT NULL, crop_width REAL NOT NULL, crop_height REAL NOT NULL,
    image_path      TEXT,
    quality_score   REAL NOT NULL DEFAULT 0,
    region_type     TEXT NOT NULL DEFAULT 'unknown',
    region_confidence REAL NOT NULL DEFAULT 0,
    feature_print   BLOB,
    is_accepted     INTEGER NOT NULL DEFAULT 0,
    origin          TEXT NOT NULL DEFAULT 'local',
    duplicate_source_ids TEXT NOT NULL DEFAULT '[]',  -- other screenshots showing the same photo
    created_at      TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_images_place ON place_images(place_id);

CREATE TABLE IF NOT EXISTS trips (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL,
    start_date  TEXT,
    end_date    TEXT,
    notes       TEXT NOT NULL DEFAULT '',
    created_at  TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS trip_places (
    id          TEXT PRIMARY KEY,
    trip_id     TEXT NOT NULL REFERENCES trips(id) ON DELETE CASCADE,
    place_id    TEXT NOT NULL REFERENCES places(id) ON DELETE CASCADE,
    position    INTEGER NOT NULL DEFAULT 0,
    day         INTEGER,
    UNIQUE (trip_id, place_id)
);

CREATE TABLE IF NOT EXISTS review_items (
    id              TEXT PRIMARY KEY,
    kind            TEXT NOT NULL,
    screenshot_id   TEXT REFERENCES screenshots(id) ON DELETE CASCADE,
    message         TEXT NOT NULL DEFAULT '',
    extracted_place TEXT,                             -- JSON ExtractedPlace
    candidates      TEXT NOT NULL DEFAULT '[]',       -- JSON [PlaceCandidate]
    place_a_id      TEXT,
    place_b_id      TEXT,
    image_id        TEXT,
    is_resolved     INTEGER NOT NULL DEFAULT 0,
    resolution      TEXT,
    created_at      TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_reviews_open ON review_items(is_resolved, kind);

-- Persistent work queue: survives restarts, prevents duplicate processing.
CREATE TABLE IF NOT EXISTS processing_jobs (
    screenshot_id   TEXT PRIMARY KEY REFERENCES screenshots(id) ON DELETE CASCADE,
    status          TEXT NOT NULL DEFAULT 'queued',   -- queued | running | done | failed
    attempts        INTEGER NOT NULL DEFAULT 0,
    last_error      TEXT,
    priority        INTEGER NOT NULL DEFAULT 0,
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL
);

-- AI results keyed by OCR content: identical input is never sent twice.
CREATE TABLE IF NOT EXISTS ai_cache (
    ocr_hash        TEXT NOT NULL,
    model           TEXT NOT NULL,
    prompt_version  INTEGER NOT NULL,
    kind            TEXT NOT NULL,
    screenshot_id   TEXT,
    result_json     TEXT NOT NULL,
    created_at      TEXT NOT NULL,
    PRIMARY KEY (ocr_hash, model, prompt_version, kind)
);

CREATE TABLE IF NOT EXISTS ai_usage_log (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    screenshot_id   TEXT,
    kind            TEXT NOT NULL,
    model           TEXT NOT NULL,
    thinking        INTEGER NOT NULL,
    image_used      INTEGER NOT NULL,
    input_tokens    INTEGER NOT NULL,
    cache_hit_tokens INTEGER NOT NULL,
    output_tokens   INTEGER NOT NULL,
    latency_ms      INTEGER NOT NULL,
    retries         INTEGER NOT NULL,
    cost            REAL NOT NULL,
    success         INTEGER NOT NULL,
    prompt_version  INTEGER NOT NULL,
    created_at      TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS metrics (
    key     TEXT PRIMARY KEY,
    value   REAL NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS settings (
    key     TEXT PRIMARY KEY,
    value   TEXT NOT NULL
);
