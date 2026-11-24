-- Spec 66: Turnover & Cleaning Crew Dispatch View, Staff Roster & Hashed Action Tokens

-- 1. Extend user_role enum with 'staff' (operational cleaning & maintenance accounts).
ALTER TYPE user_role ADD VALUE IF NOT EXISTS 'staff';

-- 2. Single-use SHA-256 hashed tokens for staff invite onboarding and self-service password resets.
CREATE TABLE account_action_token (
    id         UUID PRIMARY KEY DEFAULT uuidv7(),
    user_id    UUID NOT NULL REFERENCES "user"(id) ON DELETE CASCADE,
    purpose    TEXT NOT NULL CHECK (purpose IN ('staff_invite', 'password_reset')),
    token_hash TEXT NOT NULL UNIQUE,
    expires_at TIMESTAMPTZ NOT NULL,
    used_at    TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_account_action_token_open
    ON account_action_token (user_id, purpose)
    WHERE used_at IS NULL;

-- 3. Host-scoped staff roster & listing assignments (allows one cleaner email across multiple hosts).
CREATE TABLE host_staff (
    id            UUID PRIMARY KEY DEFAULT uuidv7(),
    host_user_id  UUID NOT NULL REFERENCES "user"(id) ON DELETE CASCADE,
    staff_user_id UUID NOT NULL REFERENCES "user"(id) ON DELETE CASCADE,
    invited_by    UUID REFERENCES "user"(id) ON DELETE SET NULL,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (host_user_id, staff_user_id)
);

CREATE INDEX idx_host_staff_staff_user ON host_staff (staff_user_id);

CREATE TABLE listing_staff (
    listing_id    UUID NOT NULL REFERENCES listing(id) ON DELETE CASCADE,
    staff_user_id UUID NOT NULL REFERENCES "user"(id) ON DELETE CASCADE,
    assigned_by   UUID REFERENCES "user"(id) ON DELETE SET NULL,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (listing_id, staff_user_id)
);

CREATE INDEX idx_listing_staff_staff_user ON listing_staff (staff_user_id);

-- 4. Turnover dispatch events (eager for bookings & iCal feeds; manual for maintenance/manual blocks).
CREATE TABLE turnover (
    id                  UUID PRIMARY KEY DEFAULT uuidv7(),
    listing_id          UUID NOT NULL REFERENCES listing(id) ON DELETE CASCADE,
    source_kind         TEXT NOT NULL CHECK (source_kind IN ('booking', 'ical_feed', 'manual')),
    booking_id          UUID REFERENCES booking(id) ON DELETE SET NULL,
    blocked_date_id     UUID REFERENCES listing_blocked_dates(id) ON DELETE SET NULL,
    source_feed_id      UUID REFERENCES listing_calendar_feed(id) ON DELETE SET NULL,
    source_external_uid TEXT,
    checkout_date       DATE NOT NULL,
    status              TEXT NOT NULL CHECK (status IN (
        'needs_cleaning',
        'cleaning_in_progress',
        'cleaning_completed',
        'maintenance_required',
        'inspected_and_ready',
        'not_required'
    )),
    title               TEXT,
    created_by          UUID REFERENCES "user"(id) ON DELETE SET NULL,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX uq_turnover_booking
    ON turnover (booking_id)
    WHERE source_kind = 'booking' AND booking_id IS NOT NULL;

CREATE UNIQUE INDEX uq_turnover_ical_source
    ON turnover (listing_id, source_feed_id, source_external_uid)
    WHERE source_kind = 'ical_feed' AND source_feed_id IS NOT NULL AND source_external_uid IS NOT NULL;

CREATE INDEX idx_turnover_listing_checkout
    ON turnover (listing_id, checkout_date, status);

-- 5. Immutable audit log of turnover status transitions.
CREATE TABLE turnover_status_history (
    id            UUID PRIMARY KEY DEFAULT uuidv7(),
    turnover_id   UUID NOT NULL REFERENCES turnover(id) ON DELETE CASCADE,
    from_status   TEXT CHECK (from_status IS NULL OR from_status IN (
        'needs_cleaning',
        'cleaning_in_progress',
        'cleaning_completed',
        'maintenance_required',
        'inspected_and_ready',
        'not_required'
    )),
    to_status     TEXT NOT NULL CHECK (to_status IN (
        'needs_cleaning',
        'cleaning_in_progress',
        'cleaning_completed',
        'maintenance_required',
        'inspected_and_ready',
        'not_required'
    )),
    actor_user_id UUID REFERENCES "user"(id) ON DELETE SET NULL,
    actor_role    TEXT NOT NULL,
    entry_point   TEXT NOT NULL,
    reason        TEXT,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_turnover_status_history_turnover
    ON turnover_status_history (turnover_id, created_at ASC);

-- 6. Isolated per-turnover chronological crew comment threads (never mixed with guest conversations).
CREATE TABLE turnover_comment (
    id             UUID PRIMARY KEY DEFAULT uuidv7(),
    turnover_id    UUID NOT NULL REFERENCES turnover(id) ON DELETE CASCADE,
    author_user_id UUID REFERENCES "user"(id) ON DELETE SET NULL,
    author_name    TEXT NOT NULL,
    author_role    TEXT NOT NULL CHECK (author_role IN ('host', 'admin', 'staff')),
    body           TEXT NOT NULL CHECK (char_length(btrim(body)) BETWEEN 1 AND 2000),
    created_at     TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_turnover_comment_turnover
    ON turnover_comment (turnover_id, created_at ASC);
