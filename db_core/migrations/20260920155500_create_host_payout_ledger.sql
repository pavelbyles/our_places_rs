-- Add property-level commission override to listing and listing_history
ALTER TABLE listing 
ADD COLUMN commission_pct DECIMAL(5, 4) NOT NULL DEFAULT 0.0000;

ALTER TABLE listing_history 
ADD COLUMN commission_pct DECIMAL(5, 4) NOT NULL DEFAULT 0.0000;

-- Create payout status enum
CREATE TYPE payout_status AS ENUM ('pending', 'processing', 'paid', 'cancelled', 'refunded');

-- Create host payout ledger table
CREATE TABLE host_payout_ledger (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    booking_id UUID NOT NULL UNIQUE REFERENCES booking(id) ON DELETE RESTRICT,
    listing_id UUID NOT NULL REFERENCES listing(id) ON DELETE RESTRICT,
    host_id UUID NOT NULL REFERENCES "user"(id) ON DELETE RESTRICT,
    currency CHAR(3) NOT NULL DEFAULT 'USD',
    gross_amount DECIMAL(12, 2) NOT NULL,
    platform_fee_pct DECIMAL(5, 4) NOT NULL DEFAULT 0.0000,
    platform_fee_amount DECIMAL(12, 2) NOT NULL DEFAULT 0.00,
    tax_withheld_amount DECIMAL(12, 2) NOT NULL DEFAULT 0.00,
    exchange_rate DECIMAL(12, 6) NOT NULL DEFAULT 1.000000,
    net_payout_amount DECIMAL(12, 2) NOT NULL,
    status payout_status NOT NULL DEFAULT 'pending',
    gateway_reference VARCHAR(255),
    failure_reason TEXT,
    payout_date TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Indexes for efficient lookups, filtering, and reconciliation
CREATE INDEX idx_host_payout_ledger_host_id ON host_payout_ledger(host_id);
CREATE INDEX idx_host_payout_ledger_listing_id ON host_payout_ledger(listing_id);
CREATE INDEX idx_host_payout_ledger_status ON host_payout_ledger(status);
CREATE INDEX idx_host_payout_ledger_booking_id ON host_payout_ledger(booking_id);
CREATE INDEX idx_host_payout_ledger_created_at ON host_payout_ledger(created_at);

-- Trigger for updated_at
CREATE TRIGGER update_host_payout_ledger_updated_at
BEFORE UPDATE ON host_payout_ledger
FOR EACH ROW
EXECUTE FUNCTION update_updated_at_column();
