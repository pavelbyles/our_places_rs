-- Create review_token table for gated access
CREATE TABLE review_token (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    token VARCHAR(64) NOT NULL UNIQUE,
    booking_id UUID NOT NULL UNIQUE REFERENCES booking(id) ON DELETE CASCADE,
    guest_id UUID NOT NULL REFERENCES "user"(id) ON DELETE CASCADE,
    listing_id UUID NOT NULL REFERENCES listing(id) ON DELETE CASCADE,
    valid_from TIMESTAMPTZ NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    used_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CHECK (valid_from < expires_at)
);

CREATE INDEX idx_review_token_hash ON review_token(token);
CREATE INDEX idx_review_token_booking_id ON review_token(booking_id);


-- Create review table
CREATE TABLE review (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    booking_id UUID NOT NULL UNIQUE REFERENCES booking(id) ON DELETE CASCADE,
    listing_id UUID NOT NULL REFERENCES listing(id) ON DELETE CASCADE,
    guest_id UUID NOT NULL REFERENCES "user"(id) ON DELETE CASCADE,
    
    -- Sub-ratings
    cleanliness_rating INTEGER NOT NULL CHECK (cleanliness_rating BETWEEN 1 AND 5),
    accuracy_rating INTEGER NOT NULL CHECK (accuracy_rating BETWEEN 1 AND 5),
    location_rating INTEGER NOT NULL CHECK (location_rating BETWEEN 1 AND 5),
    value_rating INTEGER NOT NULL CHECK (value_rating BETWEEN 1 AND 5),
    
    -- Overall derived rating
    overall_rating NUMERIC(3, 2) NOT NULL CHECK (overall_rating BETWEEN 1.00 AND 5.00),
    
    -- Text content
    public_review_text TEXT,
    private_host_feedback TEXT,
    host_reply_text TEXT,
    host_replied_at TIMESTAMPTZ,
    
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_review_listing_id ON review(listing_id);
CREATE INDEX idx_review_guest_id ON review(guest_id);
