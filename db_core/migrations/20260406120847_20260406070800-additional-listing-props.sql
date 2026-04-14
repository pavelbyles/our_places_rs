-- Add migration script here
ALTER TABLE listing 
ADD COLUMN max_guests INTEGER NOT NULL DEFAULT 1,
ADD COLUMN bedrooms INTEGER NOT NULL DEFAULT 1,
ADD COLUMN beds INTEGER NOT NULL DEFAULT 1,

-- Bathroom Breakdown (Explicitly separating powder rooms)
ADD COLUMN full_bathrooms INTEGER NOT NULL DEFAULT 1,
ADD COLUMN half_bathrooms INTEGER NOT NULL DEFAULT 0,

-- Physical & Location Specs
ADD COLUMN square_meters INTEGER,
ADD COLUMN latitude DOUBLE PRECISION,
ADD COLUMN longitude DOUBLE PRECISION,

-- Denormalized Rating Data (For O(1) sorting performance)
ADD COLUMN overall_rating NUMERIC(3, 2), -- e.g., 4.95
ADD COLUMN review_count INTEGER NOT NULL DEFAULT 0,

-- Dynamic Room Definitions (Bedrooms, Living Spaces, pool, etc. and their specific amenities)
ADD COLUMN listing_details JSONB NOT NULL DEFAULT '[]'::jsonb;

-- Create a GIN index on the room_details so we can eventually 
-- search *inside* the JSON efficiently if needed.
CREATE INDEX idx_listing_details ON listing USING GIN (listing_details);
