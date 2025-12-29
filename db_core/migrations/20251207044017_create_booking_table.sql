-- Create ENUM types for status and cancellation policy
CREATE TYPE booking_status AS ENUM ('pending', 'confirmed', 'cancelled', 'completed');
CREATE TYPE cancellation_policy AS ENUM ('flexible', 'moderate', 'strict');

-- Create the booking table
CREATE TABLE booking (
    id UUID PRIMARY KEY DEFAULT uuidv7(),

    -- Confirmation code for user-facing lookups (e.g. HMEJYC3P9B)
    confirmation_code VARCHAR(12) NOT NULL UNIQUE,

    -- Relationships
    guest_id UUID NOT NULL REFERENCES "user"(id),
    listing_id UUID NOT NULL REFERENCES listing(id),

    -- Status
    status booking_status NOT NULL DEFAULT 'pending',

    -- Dates
    date_from DATE NOT NULL,
    date_to DATE NOT NULL,

    -- Financials (Snapshot at time of booking)
    currency CHAR(3) NOT NULL DEFAULT 'USD',
    daily_rate DECIMAL(10, 2) NOT NULL,
    number_of_persons INTEGER NOT NULL,

    -- Calculations
    total_days INTEGER NOT NULL,
    sub_total_price DECIMAL(10, 2) NOT NULL,

    -- Adjustments
    discount_value DECIMAL(10, 2) DEFAULT 0.00,
    tax_value DECIMAL(10, 2) DEFAULT 0.00,

    -- Flexible storage for breakdown (Cleaning fee, Service fee, etc.)
    fee_breakdown JSONB NOT NULL DEFAULT '[]',

    -- Final amount
    total_price DECIMAL(10, 2) NOT NULL,

    -- Policy snapshot
    cancellation_policy cancellation_policy NOT NULL,

    -- Audit fields
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Create indexes for common lookups
CREATE INDEX idx_booking_guest_id ON booking(guest_id);
CREATE INDEX idx_booking_listing_id ON booking(listing_id);
CREATE INDEX idx_booking_status ON booking(status);
