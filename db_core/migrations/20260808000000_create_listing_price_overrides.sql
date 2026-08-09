-- Enable btree_gist extension for multivariable exclusion constraints
CREATE EXTENSION IF NOT EXISTS btree_gist;

CREATE TABLE listing_price_overrides (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    listing_id UUID NOT NULL REFERENCES listing(id) ON DELETE CASCADE,
    start_date DATE NOT NULL,
    end_date DATE NOT NULL,
    nightly_rate NUMERIC(12, 2) NOT NULL,
    min_nights INT NOT NULL DEFAULT 1,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT check_override_dates CHECK (end_date > start_date),
    CONSTRAINT check_override_rate CHECK (nightly_rate > 0),
    CONSTRAINT check_override_min_nights CHECK (min_nights >= 1),
    EXCLUDE USING gist (
        listing_id WITH =,
        daterange(start_date, end_date, '[)') WITH &&
    )
);

CREATE INDEX idx_listing_price_overrides_lookup 
ON listing_price_overrides(listing_id, start_date, end_date);
