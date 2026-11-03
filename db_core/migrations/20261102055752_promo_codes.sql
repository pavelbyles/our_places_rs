-- Promo codes (#106).
--
-- * Codes are stored uppercase and are unique across the platform. Their terms
--   never change after creation, so `promo_code` holds no history.
-- * A redemption is one row per booking. Usage is derived from the live status
--   of that booking, so expiry sweeps free a use without touching this table.
-- * Bookings keep a snapshot of the terms, so later changes can't reprice them.
-- * Existing bookings, ledger rows and tax documents are not changed: the new
--   columns only add defaults.

-- 0. Take every lock up front, as creating the foreign keys and then altering
--    the tables could otherwise deadlock with a running booking update.
SET LOCAL lock_timeout = '5s';
LOCK TABLE listing, "user", currencies, booking, booking_history,
           host_payout_ledger, tax_document
    IN ACCESS EXCLUSIVE MODE;

CREATE TYPE promo_discount_type AS ENUM ('percentage', 'fixed');
CREATE TYPE promo_scope AS ENUM ('stay', 'stay_and_cleaning');

CREATE TABLE promo_code (
    id                UUID PRIMARY KEY DEFAULT uuidv7(),
    code              VARCHAR(32) NOT NULL UNIQUE CHECK (code ~ '^[A-Z0-9][A-Z0-9-]{3,31}$'),
    discount_type     promo_discount_type NOT NULL,
    discount_value    NUMERIC(12,2) NOT NULL CHECK (discount_value > 0),
    currency          CHAR(3) REFERENCES currencies(code) ON DELETE RESTRICT ON UPDATE RESTRICT,
    scope             promo_scope NOT NULL,
    valid_from        TIMESTAMPTZ NOT NULL,
    valid_to          TIMESTAMPTZ NOT NULL,
    stay_from         DATE,
    stay_to           DATE,
    min_nights        INTEGER CHECK (min_nights >= 1),
    min_booking_value NUMERIC(12,2) CHECK (min_booking_value > 0),
    total_usage_limit INTEGER CHECK (total_usage_limit >= 1),
    per_guest_limit   INTEGER CHECK (per_guest_limit >= 1),
    applies_to_all    BOOLEAN NOT NULL,
    owner_host_id     UUID REFERENCES "user"(id) ON DELETE RESTRICT,
    created_by        UUID NOT NULL REFERENCES "user"(id) ON DELETE RESTRICT,
    deactivated_at    TIMESTAMPTZ,
    deactivated_by    UUID REFERENCES "user"(id) ON DELETE RESTRICT,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at        TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CHECK (valid_to > valid_from),
    CHECK (stay_from IS NULL OR stay_to IS NULL OR stay_to >= stay_from),
    CHECK (discount_type <> 'percentage' OR discount_value <= 100),
    CHECK (discount_type <> 'fixed' OR currency IS NOT NULL),
    CHECK (min_booking_value IS NULL OR currency IS NOT NULL)
);
CREATE INDEX idx_promo_code_owner ON promo_code (owner_host_id);

CREATE TRIGGER update_promo_code_updated_at
BEFORE UPDATE ON promo_code
FOR EACH ROW
EXECUTE FUNCTION update_updated_at_column();

CREATE TABLE promo_code_listing (
    promo_code_id UUID NOT NULL REFERENCES promo_code(id) ON DELETE RESTRICT,
    listing_id    UUID NOT NULL REFERENCES listing(id) ON DELETE RESTRICT,
    PRIMARY KEY (promo_code_id, listing_id)
);

CREATE TABLE promo_code_redemption (
    id            UUID PRIMARY KEY DEFAULT uuidv7(),
    promo_code_id UUID NOT NULL REFERENCES promo_code(id) ON DELETE RESTRICT,
    booking_id    UUID NOT NULL UNIQUE REFERENCES booking(id) ON DELETE RESTRICT,
    released_at   TIMESTAMPTZ,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX idx_promo_redemption_code ON promo_code_redemption (promo_code_id)
    WHERE released_at IS NULL;

ALTER TABLE booking
    ADD COLUMN promo_code_id           UUID REFERENCES promo_code(id) ON DELETE RESTRICT,
    ADD COLUMN promo_terms             JSONB,
    ADD COLUMN promo_stay_discount     NUMERIC(10,2) NOT NULL DEFAULT 0,
    ADD COLUMN promo_cleaning_discount NUMERIC(10,2) NOT NULL DEFAULT 0;

ALTER TABLE booking_history
    ADD COLUMN promo_code_id           UUID,
    ADD COLUMN promo_terms             JSONB,
    ADD COLUMN promo_stay_discount     NUMERIC(10,2),
    ADD COLUMN promo_cleaning_discount NUMERIC(10,2);

ALTER TABLE host_payout_ledger
    ADD COLUMN cleaning_fee_amount   NUMERIC(12,2) NOT NULL DEFAULT 0,
    ADD COLUMN promo_discount_amount NUMERIC(12,2) NOT NULL DEFAULT 0;

ALTER TABLE tax_document
    ADD COLUMN promo_code     VARCHAR(32),
    ADD COLUMN promo_discount NUMERIC(12,2) NOT NULL DEFAULT 0;

-- Keep the hold-expiry job's history snapshots complete. Its logic is otherwise
-- unchanged. The hold window comes from `application.pending_hold_minutes` when
-- a hold is created, and this job only reads the stored `expires_at`.
DO $$
BEGIN
    IF current_database() NOT LIKE '%test%' THEN
        BEGIN
            PERFORM cron.unschedule(jobid) FROM cron.job
            WHERE jobname = 'cleanup_stale_pending_holds';

            PERFORM cron.schedule('cleanup_stale_pending_holds', '*/15 * * * *',
                'WITH due AS (
                    SELECT id FROM booking
                    WHERE status = ''pending'' AND expires_at <= NOW()
                    FOR UPDATE SKIP LOCKED
                ), expired_bookings AS (
                    UPDATE booking b
                    SET status = ''cancelled'', updated_at = NOW()
                    FROM due
                    WHERE b.id = due.id
                    RETURNING b.*
                )
                INSERT INTO booking_history (
                    booking_id, confirmation_code, guest_id, listing_id, status,
                    date_from, date_to, currency, daily_rate, number_of_persons, total_days,
                    sub_total_price, discount_value, tax_value, fee_breakdown, total_price,
                    cancellation_policy, metadata, door_access_code,
                    cleaning_fee, damage_deposit, platform_fee, tax_rate, exchange_rate,
                    promo_code_id, promo_terms, promo_stay_discount, promo_cleaning_discount,
                    change_reason, created_at
                )
                SELECT
                    id, confirmation_code, guest_id, listing_id, status,
                    date_from, date_to, currency, daily_rate, number_of_persons, total_days,
                    sub_total_price, discount_value, tax_value, fee_breakdown, total_price,
                    cancellation_policy, metadata, door_access_code,
                    cleaning_fee, damage_deposit, platform_fee, tax_rate, exchange_rate,
                    promo_code_id, promo_terms, promo_stay_discount, promo_cleaning_discount,
                    ''Hold expired'', NOW()
                FROM expired_bookings;'
            );
        EXCEPTION WHEN OTHERS THEN
            RAISE WARNING 'pg_cron unavailable (%); expired holds will be released by booking_api run_sweeps only', SQLERRM;
        END;
    END IF;
END
$$;
