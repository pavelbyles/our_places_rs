-- 1. Listing Extra-Guest Pricing Columns
ALTER TABLE listing
    ADD COLUMN guests_included INTEGER NOT NULL DEFAULT 1 CHECK (guests_included >= 1),
    ADD COLUMN extra_guest_fee NUMERIC(12, 2) NOT NULL DEFAULT 0.00 CHECK (extra_guest_fee >= 0);

ALTER TABLE listing_history
    ADD COLUMN guests_included INTEGER,
    ADD COLUMN extra_guest_fee NUMERIC(12, 2);

-- 2. Reservation Change Request Enums & Table
CREATE TYPE reservation_change_status AS ENUM (
    'pending',
    'approved',
    'declined',
    'cancelled',
    'expired'
);

CREATE TYPE change_initiator_role AS ENUM (
    'guest',
    'host',
    'admin'
);

CREATE TYPE change_settlement_status AS ENUM (
    'not_required',
    'pending_charge',
    'charged',
    'pending_refund',
    'refunded',
    'failed'
);

CREATE TABLE reservation_change_request (
    id                         UUID PRIMARY KEY DEFAULT uuidv7(),
    booking_id                 UUID NOT NULL REFERENCES booking(id) ON DELETE CASCADE,
    listing_id                 UUID NOT NULL REFERENCES listing(id) ON DELETE CASCADE,
    initiated_by_id            UUID NOT NULL REFERENCES "user"(id),
    initiated_by_role          change_initiator_role NOT NULL,
    status                     reservation_change_status NOT NULL DEFAULT 'pending',
    previous_date_from         DATE NOT NULL,
    previous_date_to           DATE NOT NULL,
    proposed_date_from         DATE NOT NULL,
    proposed_date_to           DATE NOT NULL,
    previous_number_of_persons INTEGER NOT NULL,
    proposed_number_of_persons INTEGER NOT NULL CHECK (proposed_number_of_persons >= 1),
    proposed_num_adults        INTEGER NOT NULL CHECK (proposed_num_adults >= 1),
    proposed_num_children      INTEGER NOT NULL DEFAULT 0 CHECK (proposed_num_children >= 0),
    proposed_num_infants       INTEGER NOT NULL DEFAULT 0 CHECK (proposed_num_infants >= 0),
    proposed_num_pets          INTEGER NOT NULL DEFAULT 0 CHECK (proposed_num_pets >= 0),
    currency                   CHAR(3) NOT NULL,
    previous_total_price       NUMERIC(12, 2) NOT NULL,
    proposed_total_price       NUMERIC(12, 2) NOT NULL,
    balance_due                NUMERIC(12, 2) NOT NULL,
    previous_net_payout        NUMERIC(12, 2) NOT NULL,
    proposed_net_payout        NUMERIC(12, 2) NOT NULL,
    host_payout_delta          NUMERIC(12, 2) NOT NULL,
    pricing_snapshot           JSONB NOT NULL,
    settlement_status          change_settlement_status NOT NULL DEFAULT 'not_required',
    gateway_reference          VARCHAR(128),
    reason                     TEXT,
    responder_id               UUID REFERENCES "user"(id),
    response_note              TEXT,
    responded_at               TIMESTAMPTZ,
    expires_at_date            DATE NOT NULL,
    created_at                 TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at                 TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX uq_reservation_change_pending
    ON reservation_change_request (booking_id)
    WHERE status = 'pending';

CREATE INDEX idx_reservation_change_booking
    ON reservation_change_request (booking_id, created_at DESC);

CREATE INDEX idx_reservation_change_status_expiry
    ON reservation_change_request (status, expires_at_date)
    WHERE status = 'pending';
