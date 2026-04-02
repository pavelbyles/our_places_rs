-- Add metadata to booking
ALTER TABLE booking ADD COLUMN metadata JSONB NOT NULL DEFAULT '{}'::jsonb;

-- Add discounts to listing
ALTER TABLE listing 
ADD COLUMN weekly_discount_percentage NUMERIC DEFAULT NULL,
ADD COLUMN monthly_discount_percentage NUMERIC DEFAULT NULL;

-- Add discounts to listing_history
ALTER TABLE listing_history 
ADD COLUMN weekly_discount_percentage NUMERIC DEFAULT NULL,
ADD COLUMN monthly_discount_percentage NUMERIC DEFAULT NULL;
