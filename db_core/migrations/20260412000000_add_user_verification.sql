-- Add verification fields to user table
ALTER TABLE "user" ADD COLUMN is_verified BOOLEAN NOT NULL DEFAULT FALSE;
ALTER TABLE "user" ADD COLUMN verification_code VARCHAR(6);
ALTER TABLE "user" ADD COLUMN verification_code_expires_at TIMESTAMPTZ;

-- Add verification state to user_history table
-- Note: verification_code is not strictly required in history but good for debugging
ALTER TABLE user_history ADD COLUMN is_verified BOOLEAN NOT NULL DEFAULT FALSE;
ALTER TABLE user_history ADD COLUMN verification_code VARCHAR(6);
