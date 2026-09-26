-- v4: After-trip memories. Your own visit (date, notes) and your own photos from Photos, linked to a place.
ALTER TABLE places ADD COLUMN visited_at TEXT;
ALTER TABLE places ADD COLUMN visit_notes TEXT NOT NULL DEFAULT '';
ALTER TABLE places ADD COLUMN cover_memory_id TEXT;             -- your own photo as the cover (overrides hero_image_id)

CREATE TABLE IF NOT EXISTS place_memories (
    id              TEXT PRIMARY KEY,
    place_id        TEXT NOT NULL REFERENCES places(id) ON DELETE CASCADE,
    photos_id       TEXT NOT NULL,                    -- PHAsset.localIdentifier (the original stays in Photos)
    taken_at        TEXT,
    latitude        REAL,
    longitude       REAL,
    image_path      TEXT,                             -- local working copy (regenerable from Photos)
    thumbnail_path  TEXT,
    created_at      TEXT NOT NULL,
    UNIQUE (place_id, photos_id)
);
CREATE INDEX IF NOT EXISTS idx_memories_place ON place_memories(place_id, taken_at);
