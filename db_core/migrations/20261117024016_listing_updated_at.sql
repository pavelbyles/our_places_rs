-- Spec 102 §1.7: when a listing's public content last changed, for the
-- sitemap's <lastmod>.

-- 1. listing.updated_at. Existing rows start at added_at (their last known
--    change); the trigger is created after the backfill so it keeps that.
ALTER TABLE listing ADD COLUMN updated_at TIMESTAMPTZ;
UPDATE listing SET updated_at = added_at;
ALTER TABLE listing
    ALTER COLUMN updated_at SET NOT NULL,
    ALTER COLUMN updated_at SET DEFAULT NOW();

CREATE TRIGGER update_listing_updated_at
BEFORE UPDATE ON listing
FOR EACH ROW
EXECUTE FUNCTION update_updated_at_column();

-- 2. Photo changes count too, but are folded in when listings are read.
--    There are deliberately no triggers on listing_image: a trigger that
--    updated listing from photo writes would deadlock photo delete (which
--    deletes one photo, then promotes another) against the image worker and
--    renames, which lock a photo row first. Photo delete bumps listing
--    itself, before touching photos. Only processed originals count, so
--    uploads in progress don't move the date. GREATEST ignores NULL, so the
--    result is never NULL.
CREATE FUNCTION listing_last_modified(p_listing_id UUID, p_updated_at TIMESTAMPTZ)
RETURNS TIMESTAMPTZ
LANGUAGE sql
STABLE
AS $$
    SELECT GREATEST(
        p_updated_at,
        (SELECT MAX(li.updated_at)
           FROM listing_image li
          WHERE li.listing_id = p_listing_id
            AND li.parent_id IS NULL
            AND li.status = 'Processed')
    )
$$;
