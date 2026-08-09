-- Add deleted_at column to user table for soft delete support
ALTER TABLE "user" ADD COLUMN deleted_at TIMESTAMPTZ DEFAULT NULL;

CREATE INDEX idx_user_deleted_at ON "user"(deleted_at);
