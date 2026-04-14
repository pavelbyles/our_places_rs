-- Add migration script here
ALTER TABLE public.listing_history
    ADD COLUMN user_id uuid,
    ADD COLUMN slug text,
    ADD COLUMN max_guests integer NOT NULL DEFAULT 1,
    ADD COLUMN bedrooms integer NOT NULL DEFAULT 1,
    ADD COLUMN beds integer NOT NULL DEFAULT 1,
    ADD COLUMN full_bathrooms integer NOT NULL DEFAULT 1,
    ADD COLUMN half_bathrooms integer NOT NULL DEFAULT 0,
    ADD COLUMN square_meters integer,
    ADD COLUMN latitude double precision,
    ADD COLUMN longitude double precision,
    ADD COLUMN overall_rating numeric(3, 2),
    ADD COLUMN review_count integer NOT NULL DEFAULT 0,
    ADD COLUMN listing_details jsonb NOT NULL DEFAULT '[]'::jsonb;

UPDATE public.listing_history lh
SET 
    user_id = l.user_id,
    slug = l.slug
FROM public.listing l
WHERE lh.listing_id = l.id;

ALTER TABLE public.listing_history 
    ALTER COLUMN user_id SET NOT NULL,
    ALTER COLUMN slug SET NOT NULL;