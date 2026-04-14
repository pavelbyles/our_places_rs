ALTER TABLE public.listing ADD COLUMN slug TEXT;

-- Backfill existing listings with a unique slug
-- We use a combination of a sanitized name and a chunk of the UUID to guarantee 
-- uniqueness and ensure it doesn't accidentally match a pure UUID format.
UPDATE listing 
SET slug = 'v-' || 
           COALESCE(
               lower(regexp_replace(name, '[^a-zA-Z0-9]+', '-', 'g')), 
               'listing'
           ) || 
           '-' || 
           substring(id::text from 1 for 8)
WHERE slug IS NULL;

ALTER TABLE public.listing ALTER COLUMN slug SET NOT NULL;

ALTER TABLE public.listing ADD CONSTRAINT listing_slug_unique UNIQUE (slug);