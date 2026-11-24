-- Pending holds carry an explicit expiry. Availability treats a hold as
-- blocking only while `expires_at > NOW()`, so correctness no longer depends
-- on a background job running. The 2-hour window must match
-- `db_core::booking::PENDING_HOLD_MINUTES`.
ALTER TABLE booking ADD COLUMN expires_at TIMESTAMPTZ;

UPDATE booking
SET expires_at = created_at + INTERVAL '2 hours'
WHERE status = 'pending';

CREATE INDEX idx_booking_pending_expires_at ON booking (expires_at)
    WHERE status = 'pending';

-- Holds expired by the previous cron job were snapshotted without their money
-- fields. Those bookings went straight to `cancelled` and were never repriced,
-- so the booking row still holds the exact values at expiry.
UPDATE booking_history h
SET cleaning_fee = b.cleaning_fee,
    damage_deposit = b.damage_deposit,
    platform_fee = b.platform_fee,
    tax_rate = b.tax_rate,
    exchange_rate = b.exchange_rate
FROM booking b
WHERE h.booking_id = b.id
  AND h.change_reason LIKE 'Hold expired%'
  AND h.cleaning_fee IS NULL;

-- Housekeeping only: flip expired holds to `cancelled` with a full history
-- snapshot. booking_api's `run_sweeps` does the same, so holds are still
-- released where pg_cron is unavailable.
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
                    change_reason, created_at
                )
                SELECT
                    id, confirmation_code, guest_id, listing_id, status,
                    date_from, date_to, currency, daily_rate, number_of_persons, total_days,
                    sub_total_price, discount_value, tax_value, fee_breakdown, total_price,
                    cancellation_policy, metadata, door_access_code,
                    cleaning_fee, damage_deposit, platform_fee, tax_rate, exchange_rate,
                    ''Hold expired'', NOW()
                FROM expired_bookings;'
            );
        EXCEPTION WHEN OTHERS THEN
            RAISE WARNING 'pg_cron unavailable (%); expired holds will be released by booking_api run_sweeps only', SQLERRM;
        END;
    END IF;
END
$$;
