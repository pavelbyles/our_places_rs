-- Add the pre-fetched city column to the main listing table
ALTER TABLE public.listing ADD COLUMN city TEXT;

-- Do the same for listing_history
ALTER TABLE public.listing_history ADD COLUMN city TEXT;