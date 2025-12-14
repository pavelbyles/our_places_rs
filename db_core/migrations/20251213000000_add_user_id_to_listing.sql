DELETE FROM listing;
ALTER TABLE listing DROP COLUMN IF EXISTS user_id; -- Just in case of re-runs without transaction mgmt
ALTER TABLE listing ADD COLUMN user_id UUID NOT NULL REFERENCES "user" (id);
