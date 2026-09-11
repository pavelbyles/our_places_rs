-- Create ENUM for message sender role
CREATE TYPE message_sender_role AS ENUM ('guest', 'host', 'admin');

-- Create booking_message table
CREATE TABLE booking_message (
    id UUID PRIMARY KEY,
    booking_id UUID NOT NULL REFERENCES booking(id) ON DELETE CASCADE,
    sender_id UUID NOT NULL REFERENCES "user"(id) ON DELETE CASCADE,
    sender_role message_sender_role NOT NULL,
    sender_name TEXT NOT NULL,
    message_text TEXT NOT NULL,
    read_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Index for retrieving threads efficiently and in chronological order
CREATE INDEX idx_booking_message_thread ON booking_message(booking_id, created_at ASC);

-- Partial index for quickly querying unread messages in a booking thread
CREATE INDEX idx_booking_message_unread ON booking_message(booking_id, read_at) WHERE read_at IS NULL;
