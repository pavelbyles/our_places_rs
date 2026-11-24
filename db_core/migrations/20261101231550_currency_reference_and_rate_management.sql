-- Currency reference table, admin-managed exchange rates and their audit log (#100).
--
-- * `currencies` becomes the single source of truth for every currency code; all
--   ten currency columns reference it ON UPDATE/DELETE RESTRICT (currencies are
--   deactivated, never deleted).
-- * Column types are unchanged: CHAR(3) columns can reference the VARCHAR(3) key
--   directly (verified on PostgreSQL 18.6).
-- * `currency_exchange_rates` gains authorship, pair retirement and CHECKs.
-- * `currency_audit_log` is append-only (UPDATE, DELETE and TRUNCATE raise).

-- 0. Take every lock up front, in the order the application touches these
--    tables, so the migration cannot deadlock half-way. A timeout aborts the
--    whole (single-transaction) migration cleanly; retry in a quiet window.
SET LOCAL lock_timeout = '5s';
LOCK TABLE listing, "user", user_history, currency_exchange_rates, booking,
           booking_history, reservation_change_request, host_payout_ledger,
           tax_document
    IN ACCESS EXCLUSIVE MODE;

-- 1. Pre-checks: fail loudly, naming the offending table, column and value.
DO $$
DECLARE
    col RECORD;
    bad TEXT;
BEGIN
    FOR col IN
        SELECT * FROM (VALUES
            ('currency_exchange_rates', 'base_currency'),
            ('currency_exchange_rates', 'target_currency'),
            ('listing', 'base_currency'),
            ('user', 'default_currency'),
            ('user_history', 'default_currency'),
            ('booking', 'currency'),
            ('booking_history', 'currency'),
            ('host_payout_ledger', 'currency'),
            ('tax_document', 'currency'),
            ('reservation_change_request', 'currency')
        ) AS t(tbl, colname)
    LOOP
        EXECUTE format(
            'SELECT %1$I::text FROM %2$I WHERE upper(trim(%1$I)) !~ ''^[A-Z]{3}$'' LIMIT 1',
            col.colname, col.tbl)
        INTO bad;
        -- EXECUTE ... INTO does not set FOUND; test the value instead.
        IF bad IS NOT NULL THEN
            RAISE EXCEPTION 'Invalid currency code % in %.% (expected 3 letters A-Z)',
                quote_literal(bad), col.tbl, col.colname;
        END IF;
    END LOOP;

    SELECT format('id=%s rate=%s %s->%s', id, rate, base_currency, target_currency)
      INTO bad
      FROM currency_exchange_rates
     WHERE rate <= 0 OR upper(trim(base_currency)) = upper(trim(target_currency))
     LIMIT 1;
    IF FOUND THEN
        RAISE EXCEPTION 'Invalid row in currency_exchange_rates (rate must be > 0 and base <> target): %', bad;
    END IF;
END $$;

-- 2. Normalise codes. The WHERE guard leaves clean rows untouched, so
--    updated_at triggers (e.g. host_payout_ledger) never fire needlessly.
UPDATE currency_exchange_rates SET base_currency = upper(trim(base_currency))
 WHERE base_currency IS DISTINCT FROM upper(trim(base_currency));
UPDATE currency_exchange_rates SET target_currency = upper(trim(target_currency))
 WHERE target_currency IS DISTINCT FROM upper(trim(target_currency));
UPDATE listing SET base_currency = upper(trim(base_currency))
 WHERE base_currency IS DISTINCT FROM upper(trim(base_currency));
UPDATE "user" SET default_currency = upper(trim(default_currency))
 WHERE default_currency IS DISTINCT FROM upper(trim(default_currency));
UPDATE user_history SET default_currency = upper(trim(default_currency))
 WHERE default_currency IS DISTINCT FROM upper(trim(default_currency));
UPDATE booking SET currency = upper(trim(currency))
 WHERE currency::text IS DISTINCT FROM upper(trim(currency));
UPDATE booking_history SET currency = upper(trim(currency))
 WHERE currency::text IS DISTINCT FROM upper(trim(currency));
UPDATE host_payout_ledger SET currency = upper(trim(currency))
 WHERE currency::text IS DISTINCT FROM upper(trim(currency));
UPDATE tax_document SET currency = upper(trim(currency))
 WHERE currency::text IS DISTINCT FROM upper(trim(currency));
UPDATE reservation_change_request SET currency = upper(trim(currency))
 WHERE currency::text IS DISTINCT FROM upper(trim(currency));

-- 3. Reference table, seeded with the named defaults plus every code in use.
CREATE TABLE currencies (
    code        VARCHAR(3)  PRIMARY KEY CHECK (code ~ '^[A-Z]{3}$'),
    name        VARCHAR(64) NOT NULL CHECK (length(trim(name)) > 0),
    active      BOOLEAN     NOT NULL DEFAULT TRUE,
    created_by  UUID        NULL REFERENCES "user"(id),
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

INSERT INTO currencies (code, name) VALUES
    ('USD', 'US Dollar'),
    ('JMD', 'Jamaican Dollar'),
    ('EUR', 'Euro'),
    ('GBP', 'British Pound'),
    ('CAD', 'Canadian Dollar');

INSERT INTO currencies (code, name)
SELECT DISTINCT code, code FROM (
    SELECT base_currency::text AS code FROM currency_exchange_rates
    UNION SELECT target_currency::text FROM currency_exchange_rates
    UNION SELECT base_currency::text FROM listing
    UNION SELECT default_currency::text FROM "user"
    UNION SELECT default_currency::text FROM user_history
    UNION SELECT currency::text FROM booking
    UNION SELECT currency::text FROM booking_history
    UNION SELECT currency::text FROM host_payout_ledger
    UNION SELECT currency::text FROM tax_document
    UNION SELECT currency::text FROM reservation_change_request
) AS used
ON CONFLICT (code) DO NOTHING;

-- 4. Column types: unchanged (see header).

-- 5. Foreign keys on every currency column.
ALTER TABLE currency_exchange_rates
    ADD CONSTRAINT fk_exchange_rates_base_currency FOREIGN KEY (base_currency)
        REFERENCES currencies(code) ON UPDATE RESTRICT ON DELETE RESTRICT,
    ADD CONSTRAINT fk_exchange_rates_target_currency FOREIGN KEY (target_currency)
        REFERENCES currencies(code) ON UPDATE RESTRICT ON DELETE RESTRICT;
ALTER TABLE listing ADD CONSTRAINT fk_listing_base_currency FOREIGN KEY (base_currency)
    REFERENCES currencies(code) ON UPDATE RESTRICT ON DELETE RESTRICT;
ALTER TABLE "user" ADD CONSTRAINT fk_user_default_currency FOREIGN KEY (default_currency)
    REFERENCES currencies(code) ON UPDATE RESTRICT ON DELETE RESTRICT;
ALTER TABLE user_history ADD CONSTRAINT fk_user_history_default_currency FOREIGN KEY (default_currency)
    REFERENCES currencies(code) ON UPDATE RESTRICT ON DELETE RESTRICT;
ALTER TABLE booking ADD CONSTRAINT fk_booking_currency FOREIGN KEY (currency)
    REFERENCES currencies(code) ON UPDATE RESTRICT ON DELETE RESTRICT;
ALTER TABLE booking_history ADD CONSTRAINT fk_booking_history_currency FOREIGN KEY (currency)
    REFERENCES currencies(code) ON UPDATE RESTRICT ON DELETE RESTRICT;
ALTER TABLE host_payout_ledger ADD CONSTRAINT fk_host_payout_ledger_currency FOREIGN KEY (currency)
    REFERENCES currencies(code) ON UPDATE RESTRICT ON DELETE RESTRICT;
ALTER TABLE tax_document ADD CONSTRAINT fk_tax_document_currency FOREIGN KEY (currency)
    REFERENCES currencies(code) ON UPDATE RESTRICT ON DELETE RESTRICT;
ALTER TABLE reservation_change_request ADD CONSTRAINT fk_reservation_change_request_currency FOREIGN KEY (currency)
    REFERENCES currencies(code) ON UPDATE RESTRICT ON DELETE RESTRICT;

-- 6. Rate versions: authorship, pair retirement and integrity checks.
ALTER TABLE currency_exchange_rates
    ADD COLUMN created_by UUID NULL REFERENCES "user"(id),
    ADD COLUMN retired_at TIMESTAMPTZ NULL,
    ADD COLUMN retired_by UUID NULL REFERENCES "user"(id),
    ADD CONSTRAINT chk_exchange_rate_positive CHECK (rate > 0),
    ADD CONSTRAINT chk_exchange_rate_distinct_pair CHECK (base_currency <> target_currency);

-- 7. Append-only audit log for currency and rate administration.
CREATE TABLE currency_audit_log (
    id              UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    actor_id        UUID        NOT NULL REFERENCES "user"(id),
    action          VARCHAR(32) NOT NULL CHECK (action IN (
                        'currency_created', 'currency_updated',
                        'currency_activated', 'currency_deactivated',
                        'rate_published', 'rate_deleted',
                        'pair_retired', 'pair_reinstated')),
    currency_code   VARCHAR(3)  NULL REFERENCES currencies(code),
    base_currency   VARCHAR(3)  NULL REFERENCES currencies(code),
    target_currency VARCHAR(3)  NULL REFERENCES currencies(code),
    rate_id         UUID        NULL, -- no FK: deleted scheduled rates stay auditable
    details         JSONB       NOT NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE OR REPLACE FUNCTION currency_audit_log_append_only()
RETURNS TRIGGER AS $$
BEGIN
    RAISE EXCEPTION 'currency_audit_log is append-only (% rejected)', TG_OP;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER currency_audit_log_no_update_delete
    BEFORE UPDATE OR DELETE ON currency_audit_log
    FOR EACH ROW EXECUTE FUNCTION currency_audit_log_append_only();

CREATE TRIGGER currency_audit_log_no_truncate
    BEFORE TRUNCATE ON currency_audit_log
    FOR EACH STATEMENT EXECUTE FUNCTION currency_audit_log_append_only();

-- 8. In-use checks for deactivation / retirement scan live listings by currency.
CREATE INDEX idx_listing_base_currency_live ON listing (base_currency) WHERE deleted_at IS NULL;
