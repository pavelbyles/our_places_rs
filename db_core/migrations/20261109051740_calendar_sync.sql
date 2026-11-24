-- iCal calendar sync (#58): imported feeds, blocked dates and conflicts.

CREATE TYPE calendar_block_source  AS ENUM ('ical_feed', 'manual');
CREATE TYPE calendar_feed_status   AS ENUM ('never', 'ok', 'warning', 'failed');
CREATE TYPE calendar_conflict_kind AS ENUM ('overlap', 'turnover');

-- External .ics feeds imported into a listing. The URL contains a read token,
-- so it's treated as a secret and never logged.
CREATE TABLE listing_calendar_feed (
    id                   UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    listing_id           UUID NOT NULL REFERENCES listing(id) ON DELETE CASCADE,
    name                 VARCHAR(60)  NOT NULL CHECK (length(btrim(name)) > 0),
    url                  TEXT         NOT NULL CHECK (length(url) <= 2048),
    is_active            BOOLEAN      NOT NULL DEFAULT TRUE,
    sync_generation      INTEGER      NOT NULL DEFAULT 0,
    last_status          calendar_feed_status NOT NULL DEFAULT 'never',
    last_error           TEXT,
    last_warning         TEXT,
    last_attempt_at      TIMESTAMPTZ,
    last_success_at      TIMESTAMPTZ,
    consecutive_failures INTEGER      NOT NULL DEFAULT 0 CHECK (consecutive_failures >= 0),
    failure_alerted_at   TIMESTAMPTZ,
    event_count          INTEGER      NOT NULL DEFAULT 0 CHECK (event_count >= 0),
    created_by           UUID REFERENCES "user"(id) ON DELETE SET NULL,
    created_at           TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    updated_at           TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    UNIQUE (listing_id, url),
    -- Target for the composite foreign keys below.
    UNIQUE (id, listing_id)
);
CREATE INDEX idx_calendar_feed_due ON listing_calendar_feed (last_attempt_at NULLS FIRST, id) WHERE is_active;

-- Nights [date_from, date_to) a listing can't be booked, imported or manual.
CREATE TABLE listing_blocked_dates (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    listing_id   UUID NOT NULL REFERENCES listing(id) ON DELETE CASCADE,
    source       calendar_block_source NOT NULL,
    feed_id      UUID,
    external_uid TEXT CHECK (length(external_uid) <= 512),
    date_from    DATE NOT NULL,
    date_to      DATE NOT NULL CHECK (date_to > date_from),
    note         VARCHAR(500),
    created_by   UUID REFERENCES "user"(id) ON DELETE SET NULL,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    FOREIGN KEY (feed_id, listing_id) REFERENCES listing_calendar_feed (id, listing_id) ON DELETE CASCADE,
    CHECK (
      (source = 'ical_feed' AND feed_id IS NOT NULL AND external_uid IS NOT NULL AND note IS NULL)
      OR (source = 'manual' AND feed_id IS NULL AND external_uid IS NULL)
    ),
    UNIQUE (feed_id, external_uid)
);
CREATE INDEX idx_blocked_dates_listing_range ON listing_blocked_dates (listing_id, date_from, date_to);
CREATE INDEX idx_blocked_dates_listing_to    ON listing_blocked_dates (listing_id, date_to);
CREATE UNIQUE INDEX uq_blocked_dates_manual  ON listing_blocked_dates (listing_id, date_from, date_to) WHERE source = 'manual';

-- An imported block landing on a stay or live hold. Kept until acknowledged.
CREATE TABLE calendar_conflict (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    listing_id      UUID NOT NULL REFERENCES listing(id) ON DELETE CASCADE,
    feed_id         UUID,
    -- Kept for the audit trail after the feed is deleted.
    feed_name       VARCHAR(60) NOT NULL,
    external_uid    TEXT NOT NULL,
    block_id        UUID REFERENCES listing_blocked_dates(id) ON DELETE SET NULL,
    booking_id      UUID NOT NULL REFERENCES booking(id) ON DELETE CASCADE,
    kind            calendar_conflict_kind NOT NULL,
    block_date_from DATE NOT NULL,
    block_date_to   DATE NOT NULL,
    detected_at     TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    reopened_at     TIMESTAMPTZ,
    acknowledged_at TIMESTAMPTZ,
    acknowledged_by UUID REFERENCES "user"(id) ON DELETE SET NULL,
    FOREIGN KEY (feed_id, listing_id) REFERENCES listing_calendar_feed (id, listing_id) ON DELETE SET NULL (feed_id),
    CHECK (acknowledged_by IS NULL OR acknowledged_at IS NOT NULL),
    -- NULL feed_id rows (feed deleted) never collide with new ones.
    UNIQUE (feed_id, external_uid, booking_id)
);
CREATE INDEX idx_calendar_conflict_open ON calendar_conflict (listing_id) WHERE acknowledged_at IS NULL;
CREATE INDEX idx_calendar_conflict_history ON calendar_conflict (listing_id, acknowledged_at DESC) WHERE acknowledged_at IS NOT NULL;
