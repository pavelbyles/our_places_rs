-- Add door_access_code to booking and booking_history
ALTER TABLE booking ADD COLUMN door_access_code TEXT DEFAULT NULL;
ALTER TABLE booking_history ADD COLUMN door_access_code TEXT DEFAULT NULL;

-- Create notification tracking log for multi-party idempotency
CREATE TABLE booking_notification_log (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    booking_id UUID NOT NULL REFERENCES booking(id) ON DELETE CASCADE,
    notification_type VARCHAR(64) NOT NULL,
    recipient_user_id UUID NOT NULL REFERENCES "user"(id) ON DELETE CASCADE,
    sent_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT uq_booking_notification UNIQUE (booking_id, notification_type, recipient_user_id)
);

CREATE INDEX idx_booking_notification_lookup 
ON booking_notification_log (booking_id, notification_type);

CREATE INDEX idx_booking_notification_recipient 
ON booking_notification_log (recipient_user_id);

-- Update pending hold cleanup cron to 2 hours
DO $$
BEGIN
    IF current_database() NOT LIKE '%test%' THEN
        BEGIN
            PERFORM cron.unschedule('cleanup_stale_pending_holds');
            PERFORM cron.schedule('cleanup_stale_pending_holds', '*/15 * * * *', 
                $cron$WITH expired_bookings AS (
                    UPDATE booking
                    SET status = 'cancelled', updated_at = NOW()
                    WHERE status = 'pending' 
                      AND created_at < NOW() - INTERVAL '2 hours'
                    RETURNING *
                )
                INSERT INTO booking_history (
                    booking_id, confirmation_code, guest_id, listing_id, status,
                    date_from, date_to, currency, daily_rate, number_of_persons, total_days,
                    sub_total_price, discount_value, tax_value, fee_breakdown, total_price,
                    cancellation_policy, metadata, door_access_code, change_reason, created_at
                )
                SELECT 
                    id, confirmation_code, guest_id, listing_id, status,
                    date_from, date_to, currency, daily_rate, number_of_persons, total_days,
                    sub_total_price, discount_value, tax_value, fee_breakdown, total_price,
                    cancellation_policy, metadata, door_access_code, 'Hold expired after 2 hours', NOW()
                FROM expired_bookings;$cron$
            );
        EXCEPTION WHEN OTHERS THEN
            RAISE NOTICE 'Skipping pg_cron adjustment: %', SQLERRM;
        END;
    END IF;
END
$$;
