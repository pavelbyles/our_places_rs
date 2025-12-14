-- Add deleted_at column to listing table for soft deletes
ALTER TABLE listing ADD COLUMN deleted_at TIMESTAMPTZ;
