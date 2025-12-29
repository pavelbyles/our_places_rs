-- Remove the unique constraint on name in listing_history
ALTER TABLE listing_history DROP CONSTRAINT listing_history_name_key;
