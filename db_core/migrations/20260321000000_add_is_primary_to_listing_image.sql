-- Add is_primary to listing_image table
ALTER TABLE listing_image ADD COLUMN is_primary BOOLEAN NOT NULL DEFAULT FALSE;
