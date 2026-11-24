-- #105: enquiries and requests to book.
--
-- A listing sells instantly, on request, or both. Enquiries start a thread and
-- block nothing. Requests (and the offers they become once accepted or quoted)
-- block their dates while live: status open/accepted/quoted and
-- `expires_at > NOW()`. Availability reads enforce that on every query, so the
-- sweep in booking_api's run_sweeps is housekeeping only.

-- 1. Booking mode per listing. Existing listings keep instant booking.
CREATE TYPE booking_mode AS ENUM ('instant', 'request', 'both');
ALTER TABLE listing ADD COLUMN booking_mode booking_mode NOT NULL DEFAULT 'instant';
ALTER TABLE listing_history ADD COLUMN booking_mode booking_mode;

-- 2. One conversation per (listing, guest): enquiries, requests, quotes and
-- the bookings for that pair share one thread.
CREATE TABLE conversation (
    id              UUID PRIMARY KEY DEFAULT uuidv7(),
    listing_id      UUID NOT NULL REFERENCES listing(id) ON DELETE CASCADE,
    guest_id        UUID NOT NULL REFERENCES "user"(id) ON DELETE CASCADE,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_message_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT uq_conversation_pair UNIQUE (listing_id, guest_id)
);
CREATE INDEX idx_conversation_guest ON conversation (guest_id, last_message_at DESC);

-- 3. Enquiries: a question with dates and a guest count. Never blocks dates.
CREATE TYPE enquiry_status AS ENUM ('open', 'closed');
CREATE TYPE enquiry_closed_reason AS ENUM (
    'request_submitted', 'quoted', 'closed_by_host', 'booked', 'expired', 'superseded'
);
CREATE TABLE booking_enquiry (
    id                UUID PRIMARY KEY DEFAULT uuidv7(),
    conversation_id   UUID NOT NULL REFERENCES conversation(id) ON DELETE CASCADE,
    listing_id        UUID NOT NULL REFERENCES listing(id) ON DELETE CASCADE,
    guest_id          UUID NOT NULL REFERENCES "user"(id) ON DELETE CASCADE,
    date_from         DATE NOT NULL,
    date_to           DATE NOT NULL,
    number_of_persons INTEGER NOT NULL CHECK (number_of_persons >= 1),
    currency          VARCHAR(3) NOT NULL REFERENCES currencies(code) ON UPDATE RESTRICT ON DELETE RESTRICT,
    status            enquiry_status NOT NULL DEFAULT 'open',
    closed_reason     enquiry_closed_reason,
    first_response_at TIMESTAMPTZ,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at        TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT chk_enquiry_dates CHECK (date_to > date_from),
    CONSTRAINT chk_enquiry_closed_reason CHECK ((status = 'closed') = (closed_reason IS NOT NULL))
);
CREATE UNIQUE INDEX uq_enquiry_open_pair ON booking_enquiry (listing_id, guest_id) WHERE status = 'open';
CREATE INDEX idx_enquiry_open_listing ON booking_enquiry (listing_id, created_at) WHERE status = 'open';
CREATE INDEX idx_enquiry_guest ON booking_enquiry (guest_id, created_at DESC);

-- 4. Requests to book, priced and snapshotted at submission.
CREATE TYPE booking_request_status AS ENUM (
    'open', 'accepted', 'quoted', 'declined', 'expired', 'withdrawn', 'cancelled', 'converted'
);
CREATE TYPE booking_request_origin AS ENUM ('guest_request', 'enquiry_quote');
CREATE TYPE decline_reason AS ENUM (
    'dates_unavailable', 'group_size', 'stay_length', 'event_not_permitted', 'other'
);
CREATE TABLE booking_request (
    id                UUID PRIMARY KEY DEFAULT uuidv7(),
    conversation_id   UUID NOT NULL REFERENCES conversation(id) ON DELETE CASCADE,
    listing_id        UUID NOT NULL REFERENCES listing(id) ON DELETE CASCADE,
    guest_id          UUID NOT NULL REFERENCES "user"(id) ON DELETE CASCADE,
    enquiry_id        UUID REFERENCES booking_enquiry(id) ON DELETE SET NULL,
    origin            booking_request_origin NOT NULL,
    status            booking_request_status NOT NULL DEFAULT 'open',
    date_from         DATE NOT NULL,
    date_to           DATE NOT NULL,
    number_of_persons INTEGER NOT NULL CHECK (number_of_persons >= 1),
    currency          VARCHAR(3) NOT NULL REFERENCES currencies(code) ON UPDATE RESTRICT ON DELETE RESTRICT,
    -- common::booking_request::StayPriceSnapshot, as requested (amounts as strings).
    price_snapshot    JSONB NOT NULL,
    -- Guest breakdown and arrival details, copied to the booking at conversion.
    metadata          JSONB NOT NULL DEFAULT '{}'::jsonb,
    -- A live open/accepted request with a code reserves a use of it (derived).
    promo_code_id     UUID REFERENCES promo_code(id) ON DELETE RESTRICT,
    current_quote_id  UUID,
    expires_at        TIMESTAMPTZ NOT NULL,
    first_response_at TIMESTAMPTZ,
    decline_reason    decline_reason,
    decline_note      TEXT CHECK (char_length(decline_note) <= 1000),
    -- Set on conversion. Not a CHECK on status: a feature-flagged hard delete
    -- of the booking sets it NULL.
    booking_id        UUID REFERENCES booking(id) ON DELETE SET NULL,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at        TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT chk_request_dates CHECK (date_to > date_from),
    CONSTRAINT chk_request_decline CHECK ((status = 'declined') = (decline_reason IS NOT NULL))
);
-- One live request per guest and listing. Lapsed-but-unswept rows are expired
-- inline before an insert, because a partial index can't depend on NOW().
CREATE UNIQUE INDEX uq_request_live_pair ON booking_request (listing_id, guest_id)
    WHERE status IN ('open', 'accepted', 'quoted');
CREATE INDEX idx_request_live_dates ON booking_request (listing_id, date_from, date_to, expires_at)
    WHERE status IN ('open', 'accepted', 'quoted');
CREATE INDEX idx_request_live_expiry ON booking_request (expires_at)
    WHERE status IN ('open', 'accepted', 'quoted');
CREATE INDEX idx_request_promo_live ON booking_request (promo_code_id)
    WHERE status IN ('open', 'accepted') AND promo_code_id IS NOT NULL;
CREATE INDEX idx_request_guest ON booking_request (guest_id, created_at DESC);
CREATE INDEX idx_request_listing_created ON booking_request (listing_id, created_at);

-- 5. Custom quotes. A newer quote supersedes the previous one.
CREATE TABLE booking_request_quote (
    id              UUID PRIMARY KEY DEFAULT uuidv7(),
    request_id      UUID NOT NULL REFERENCES booking_request(id) ON DELETE CASCADE,
    stay_price_base NUMERIC(12, 2) NOT NULL CHECK (stay_price_base > 0),
    terms           TEXT NOT NULL DEFAULT '' CHECK (char_length(terms) <= 2000),
    -- The quoted price (promo-free), as a StayPriceSnapshot.
    price_snapshot  JSONB NOT NULL,
    valid_until     TIMESTAMPTZ NOT NULL,
    superseded_at   TIMESTAMPTZ,
    created_by      UUID NOT NULL REFERENCES "user"(id),
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX idx_request_quote_request ON booking_request_quote (request_id, created_at);
ALTER TABLE booking_request ADD CONSTRAINT fk_request_current_quote
    FOREIGN KEY (current_quote_id) REFERENCES booking_request_quote(id) ON DELETE SET NULL;

-- 6. Immutable lifecycle log for enquiries and requests. Append-only: code
-- never updates or deletes these rows.
CREATE TABLE booking_request_history (
    id          UUID PRIMARY KEY DEFAULT uuidv7(),
    enquiry_id  UUID REFERENCES booking_enquiry(id) ON DELETE CASCADE,
    request_id  UUID REFERENCES booking_request(id) ON DELETE CASCADE,
    from_status TEXT,
    to_status   TEXT NOT NULL,
    quote_id    UUID REFERENCES booking_request_quote(id) ON DELETE SET NULL,
    -- NULL means the system (a sweep or supersession by another action).
    actor_id    UUID REFERENCES "user"(id) ON DELETE SET NULL,
    actor_role  message_sender_role,
    reason      TEXT NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT chk_request_history_subject CHECK (num_nonnulls(enquiry_id, request_id) = 1)
);
CREATE INDEX idx_request_history_request ON booking_request_history (request_id, created_at);
CREATE INDEX idx_request_history_enquiry ON booking_request_history (enquiry_id, created_at);

-- 7. Scheduled request emails (expiry, host reminder) are claimed here first,
-- so a re-run sweep never sends twice.
CREATE TABLE request_notification_log (
    id                UUID PRIMARY KEY DEFAULT uuidv7(),
    request_id        UUID NOT NULL REFERENCES booking_request(id) ON DELETE CASCADE,
    notification_type VARCHAR(64) NOT NULL,
    recipient_user_id UUID NOT NULL REFERENCES "user"(id) ON DELETE CASCADE,
    sent_at           TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT uq_request_notification UNIQUE (request_id, notification_type, recipient_user_id)
);

-- 8. Re-anchor booking messages on conversations.
INSERT INTO conversation (listing_id, guest_id, created_at, last_message_at)
SELECT b.listing_id, b.guest_id, MIN(m.created_at), MAX(m.created_at)
FROM booking_message m
JOIN booking b ON b.id = m.booking_id
GROUP BY b.listing_id, b.guest_id
ON CONFLICT (listing_id, guest_id) DO NOTHING;

ALTER TABLE booking_message
    ADD COLUMN conversation_id UUID REFERENCES conversation(id) ON DELETE CASCADE,
    ADD COLUMN enquiry_id UUID REFERENCES booking_enquiry(id) ON DELETE SET NULL,
    ADD COLUMN request_id UUID REFERENCES booking_request(id) ON DELETE SET NULL;

UPDATE booking_message m
SET conversation_id = c.id
FROM booking b
JOIN conversation c ON c.listing_id = b.listing_id AND c.guest_id = b.guest_id
WHERE b.id = m.booking_id;

ALTER TABLE booking_message
    ALTER COLUMN conversation_id SET NOT NULL,
    ALTER COLUMN booking_id DROP NOT NULL;

CREATE INDEX idx_booking_message_conversation ON booking_message (conversation_id, created_at);
CREATE INDEX idx_booking_message_conv_unread ON booking_message (conversation_id) WHERE read_at IS NULL;
