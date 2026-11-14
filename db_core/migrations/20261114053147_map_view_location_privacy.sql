-- Spec 103 §2.1: what guests may see of a listing's location before they book.
-- Every listing, existing and new, starts as 'approximate'.
CREATE TYPE location_privacy AS ENUM ('approximate', 'exact');

ALTER TABLE listing
    ADD COLUMN location_privacy location_privacy NOT NULL DEFAULT 'approximate';

-- History rows written before this migration stay NULL: unknown, not 'approximate'.
ALTER TABLE listing_history
    ADD COLUMN location_privacy location_privacy;
