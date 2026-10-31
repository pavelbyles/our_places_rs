-- Public, unique, user-editable handle for each user (e.g. "jane-brown"),
-- used in links such as /listings?host=jane-brown. It defaults to
-- "<first>-<last>" and stays put when the name changes, so links keep working.

-- Lowercase ASCII letters and digits joined by single hyphens, at most 60
-- characters, with common accented Latin letters folded (José -> jose).
-- Mirrors common::slug::slugify.
CREATE OR REPLACE FUNCTION user_slug_base(first_name TEXT, last_name TEXT)
RETURNS TEXT
LANGUAGE sql
IMMUTABLE
AS $$
    SELECT COALESCE(
        NULLIF(
            trim(BOTH '-' FROM left(
                trim(BOTH '-' FROM regexp_replace(
                    translate(
                        lower(coalesce(first_name, '') || '-' || coalesce(last_name, '')),
                        'àáâãäåāèéêëēìíîïīòóôõöøōùúûüūýÿñç',
                        'aaaaaaaeeeeeiiiiiooooooouuuuuyync'
                    ),
                    '[^a-z0-9]+', '-', 'g'
                )),
                60
            )),
            ''
        ),
        'user'
    );
$$;

-- `base`, or `base-2`, `base-3`, ... whichever is free. Bases shorter than
-- 3 characters (e.g. "a-b" is fine, "ab" from odd names is not) get "-user".
CREATE OR REPLACE FUNCTION unique_user_slug(base TEXT)
RETURNS TEXT
LANGUAGE plpgsql
AS $$
DECLARE
    stem TEXT := CASE WHEN char_length(base) < 3 THEN base || '-user' ELSE base END;
    candidate TEXT := stem;
    n INT := 1;
BEGIN
    -- Serialises concurrent inserts that want the same stem until commit,
    -- so two "Jane Brown" sign-ups cannot both pick "jane-brown".
    PERFORM pg_advisory_xact_lock(hashtext('user_slug:' || stem));
    WHILE EXISTS (SELECT 1 FROM "user" WHERE slug = candidate) LOOP
        n := n + 1;
        candidate := rtrim(left(stem, 60 - char_length(n::text) - 1), '-') || '-' || n;
    END LOOP;
    RETURN candidate;
END;
$$;

ALTER TABLE "user" ADD COLUMN slug TEXT;

-- Backfill oldest accounts first, so the earliest "Jane Brown" keeps "jane-brown".
DO $$
DECLARE
    u RECORD;
BEGIN
    FOR u IN SELECT id, first_name, last_name FROM "user" ORDER BY created_at, id LOOP
        UPDATE "user"
        SET slug = unique_user_slug(user_slug_base(u.first_name, u.last_name))
        WHERE id = u.id;
    END LOOP;
END;
$$;

ALTER TABLE "user" ALTER COLUMN slug SET NOT NULL;
ALTER TABLE "user" ADD CONSTRAINT user_slug_unique UNIQUE (slug);
ALTER TABLE "user" ADD CONSTRAINT user_slug_format
    CHECK (slug ~ '^[a-z0-9]+(-[a-z0-9]+)*$' AND char_length(slug) BETWEEN 3 AND 60);

-- Every insert path (API, seeds, test fixtures) gets a default slug.
CREATE OR REPLACE FUNCTION user_default_slug()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    IF NEW.slug IS NULL OR btrim(NEW.slug) = '' THEN
        NEW.slug := unique_user_slug(user_slug_base(NEW.first_name, NEW.last_name));
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER user_default_slug
    BEFORE INSERT ON "user"
    FOR EACH ROW
    EXECUTE FUNCTION user_default_slug();
