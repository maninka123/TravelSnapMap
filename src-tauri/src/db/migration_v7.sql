-- v7: corrections that survive re-processing, and indexes for the lookups the app makes most.

-- "This name on this screenshot/Reel is not a place" (removed pin, dismissed review). Re-processing skips it.
CREATE TABLE IF NOT EXISTS source_decisions (
    id              TEXT PRIMARY KEY,
    screenshot_id   TEXT REFERENCES screenshots(id) ON DELETE CASCADE,
    reel_id         TEXT REFERENCES reels(id) ON DELETE CASCADE,
    extracted_name  TEXT NOT NULL,
    name_key        TEXT NOT NULL,                    -- normalised name, for matching
    decision        TEXT NOT NULL DEFAULT 'rejected',
    created_at      TEXT NOT NULL
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_decisions_source
    ON source_decisions(COALESCE(screenshot_id, ''), COALESCE(reel_id, ''), name_key);

-- Map identifiers that now mean another place: the place merged into it, or the wrong location you corrected.
CREATE TABLE IF NOT EXISTS place_aliases (
    map_identifier  TEXT PRIMARY KEY,
    place_id        TEXT NOT NULL REFERENCES places(id) ON DELETE CASCADE,
    reason          TEXT NOT NULL,                    -- merged | relocated
    created_at      TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_aliases_place ON place_aliases(place_id);

-- Foreign keys that are filtered or cascaded on but had no index.
CREATE INDEX IF NOT EXISTS idx_facts_screenshot ON travel_facts(screenshot_id);
CREATE INDEX IF NOT EXISTS idx_facts_reel ON travel_facts(reel_id);
CREATE INDEX IF NOT EXISTS idx_images_screenshot ON place_images(screenshot_id);
CREATE INDEX IF NOT EXISTS idx_images_reel ON place_images(reel_id);
CREATE INDEX IF NOT EXISTS idx_trip_places_trip ON trip_places(trip_id, day, position);
CREATE INDEX IF NOT EXISTS idx_trip_places_place ON trip_places(place_id);
CREATE INDEX IF NOT EXISTS idx_reviews_screenshot ON review_items(screenshot_id);
CREATE INDEX IF NOT EXISTS idx_reviews_reel ON review_items(reel_id);
CREATE INDEX IF NOT EXISTS idx_places_country ON places(country_code);
