-- This migration is a fixup for schemas that were migrated while 
-- some migrations had hardcoded 'public.' schema prefixes.
-- It ensures all expected columns exist in the 'listing' and 
-- 'listing_history' tables in the CURRENT schema.

DO $$
BEGIN
    -- Fix 'listing' table
    IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'listing') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'listing' AND column_name = 'max_guests') THEN
            ALTER TABLE listing ADD COLUMN max_guests INTEGER NOT NULL DEFAULT 1;
        END IF;
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'listing' AND column_name = 'bedrooms') THEN
            ALTER TABLE listing ADD COLUMN bedrooms INTEGER NOT NULL DEFAULT 1;
        END IF;
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'listing' AND column_name = 'beds') THEN
            ALTER TABLE listing ADD COLUMN beds INTEGER NOT NULL DEFAULT 1;
        END IF;
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'listing' AND column_name = 'full_bathrooms') THEN
            ALTER TABLE listing ADD COLUMN full_bathrooms INTEGER NOT NULL DEFAULT 1;
        END IF;
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'listing' AND column_name = 'half_bathrooms') THEN
            ALTER TABLE listing ADD COLUMN half_bathrooms INTEGER NOT NULL DEFAULT 0;
        END IF;
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'listing' AND column_name = 'square_meters') THEN
            ALTER TABLE listing ADD COLUMN square_meters INTEGER;
        END IF;
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'listing' AND column_name = 'latitude') THEN
            ALTER TABLE listing ADD COLUMN latitude DOUBLE PRECISION;
        END IF;
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'listing' AND column_name = 'longitude') THEN
            ALTER TABLE listing ADD COLUMN longitude DOUBLE PRECISION;
        END IF;
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'listing' AND column_name = 'overall_rating') THEN
            ALTER TABLE listing ADD COLUMN overall_rating NUMERIC(3, 2);
        END IF;
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'listing' AND column_name = 'review_count') THEN
            ALTER TABLE listing ADD COLUMN review_count INTEGER NOT NULL DEFAULT 0;
        END IF;
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'listing' AND column_name = 'listing_details') THEN
            ALTER TABLE listing ADD COLUMN listing_details JSONB NOT NULL DEFAULT '[]'::jsonb;
        END IF;
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'listing' AND column_name = 'slug') THEN
            ALTER TABLE listing ADD COLUMN slug TEXT;
        END IF;
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'listing' AND column_name = 'city') THEN
            ALTER TABLE listing ADD COLUMN city TEXT;
        END IF;
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'listing' AND column_name = 'minimum_stay') THEN
            ALTER TABLE listing ADD COLUMN minimum_stay INTEGER NOT NULL DEFAULT 1;
        END IF;
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'listing' AND column_name = 'days_between_bookings') THEN
            ALTER TABLE listing ADD COLUMN days_between_bookings INTEGER NOT NULL DEFAULT 0;
        END IF;
    END IF;

    -- Fix 'listing_history' table
    IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'listing_history') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'listing_history' AND column_name = 'max_guests') THEN
            ALTER TABLE listing_history ADD COLUMN max_guests INTEGER NOT NULL DEFAULT 1;
        END IF;
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'listing_history' AND column_name = 'bedrooms') THEN
            ALTER TABLE listing_history ADD COLUMN bedrooms INTEGER NOT NULL DEFAULT 1;
        END IF;
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'listing_history' AND column_name = 'beds') THEN
            ALTER TABLE listing_history ADD COLUMN beds INTEGER NOT NULL DEFAULT 1;
        END IF;
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'listing_history' AND column_name = 'full_bathrooms') THEN
            ALTER TABLE listing_history ADD COLUMN full_bathrooms INTEGER NOT NULL DEFAULT 1;
        END IF;
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'listing_history' AND column_name = 'half_bathrooms') THEN
            ALTER TABLE listing_history ADD COLUMN half_bathrooms INTEGER NOT NULL DEFAULT 0;
        END IF;
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'listing_history' AND column_name = 'square_meters') THEN
            ALTER TABLE listing_history ADD COLUMN square_meters INTEGER;
        END IF;
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'listing_history' AND column_name = 'latitude') THEN
            ALTER TABLE listing_history ADD COLUMN latitude DOUBLE PRECISION;
        END IF;
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'listing_history' AND column_name = 'longitude') THEN
            ALTER TABLE listing_history ADD COLUMN longitude DOUBLE PRECISION;
        END IF;
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'listing_history' AND column_name = 'overall_rating') THEN
            ALTER TABLE listing_history ADD COLUMN overall_rating NUMERIC(3, 2);
        END IF;
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'listing_history' AND column_name = 'review_count') THEN
            ALTER TABLE listing_history ADD COLUMN review_count INTEGER NOT NULL DEFAULT 0;
        END IF;
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'listing_history' AND column_name = 'listing_details') THEN
            ALTER TABLE listing_history ADD COLUMN listing_details JSONB NOT NULL DEFAULT '[]'::jsonb;
        END IF;
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'listing_history' AND column_name = 'slug') THEN
            ALTER TABLE listing_history ADD COLUMN slug TEXT;
        END IF;
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'listing_history' AND column_name = 'city') THEN
            ALTER TABLE listing_history ADD COLUMN city TEXT;
        END IF;
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'listing_history' AND column_name = 'minimum_stay') THEN
            ALTER TABLE listing_history ADD COLUMN minimum_stay INTEGER NOT NULL DEFAULT 1;
        END IF;
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'listing_history' AND column_name = 'days_between_bookings') THEN
            ALTER TABLE listing_history ADD COLUMN days_between_bookings INTEGER NOT NULL DEFAULT 0;
        END IF;
    END IF;
END $$;
