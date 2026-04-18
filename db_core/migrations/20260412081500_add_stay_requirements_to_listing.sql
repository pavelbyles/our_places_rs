-- Add migration script here
ALTER TABLE listing 
ADD COLUMN minimum_stay INTEGER NOT NULL DEFAULT 1,
ADD COLUMN days_between_bookings INTEGER NOT NULL DEFAULT 0;

ALTER TABLE listing_history
ADD COLUMN minimum_stay INTEGER NOT NULL DEFAULT 1,
ADD COLUMN days_between_bookings INTEGER NOT NULL DEFAULT 0;
