-- Create the booking history table to track full snapshots of changes
CREATE TABLE IF NOT EXISTS booking_history (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    booking_id UUID NOT NULL REFERENCES booking(id) ON DELETE CASCADE,
    
    -- Snapshot fields from booking
    confirmation_code VARCHAR(12) NOT NULL,
    guest_id UUID NOT NULL,
    listing_id UUID NOT NULL,
    status booking_status NOT NULL,
    date_from DATE NOT NULL,
    date_to DATE NOT NULL,
    currency CHAR(3) NOT NULL,
    daily_rate DECIMAL(10, 2) NOT NULL,
    number_of_persons INTEGER NOT NULL,
    total_days INTEGER NOT NULL,
    sub_total_price DECIMAL(10, 2) NOT NULL,
    discount_value DECIMAL(10, 2),
    tax_value DECIMAL(10, 2),
    fee_breakdown JSONB NOT NULL,
    total_price DECIMAL(10, 2) NOT NULL,
    cancellation_policy cancellation_policy NOT NULL,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,

    -- History meta fields
    changed_by_id UUID REFERENCES "user"(id),
    change_reason TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Index for performance
CREATE INDEX idx_booking_history_booking_id ON booking_history(booking_id);
