-- Allow soft-deleted listings to release their slugs for reuse by active listings.
ALTER TABLE listing DROP CONSTRAINT listing_slug_unique;
CREATE UNIQUE INDEX listing_slug_unique ON listing (slug) WHERE deleted_at IS NULL;
