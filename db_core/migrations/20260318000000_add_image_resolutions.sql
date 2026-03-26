-- Add image_resolution enum
CREATE TYPE image_resolution AS ENUM (
    'Raw',
    'Thumbnail400w',
    'Mobile720w',
    'Tablet1280w',
    'Desktop1920w',
    'HighRes2560w'
);

-- Add resolution and parent_id columns to listing_image
ALTER TABLE listing_image
ADD COLUMN resolution image_resolution NOT NULL DEFAULT 'Raw',
ADD COLUMN parent_id UUID REFERENCES listing_image(id) ON DELETE CASCADE;

-- Create an index on parent_id to speed up queries for variants
CREATE INDEX idx_listing_image_parent_id ON listing_image(parent_id);
