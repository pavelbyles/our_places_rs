CREATE TABLE listing_history (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    listing_id UUID NOT NULL REFERENCES listing(id) ON DELETE CASCADE,

    name TEXT NOT NULL UNIQUE,
    description TEXT,
    listing_structure_id INTEGER NOT NULL,
    country VARCHAR(255) NOT NULL,
    price_per_night DECIMAL,
    is_active BOOLEAN NOT NULL,

    -- Audit fields
    valid_from TIMESTAMPTZ NOT NULL, -- When this version started
    archived_at TIMESTAMPTZ NOT NULL DEFAULT now() -- When this version was replaced
);

-- Index for fast history lookups
CREATE INDEX idx_listing_history_listing_id ON listing_history(listing_id);
