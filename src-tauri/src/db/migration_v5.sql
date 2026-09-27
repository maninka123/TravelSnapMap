-- v5: Recently reviewed. When each review was answered and which place was chosen, so an answer can be changed later.
ALTER TABLE review_items ADD COLUMN resolved_at TEXT;
ALTER TABLE review_items ADD COLUMN resolved_place_id TEXT;
CREATE INDEX IF NOT EXISTS idx_reviews_resolved ON review_items(is_resolved, resolved_at);
-- Your own photos can carry a caption / details.
ALTER TABLE place_images ADD COLUMN caption TEXT NOT NULL DEFAULT '';
