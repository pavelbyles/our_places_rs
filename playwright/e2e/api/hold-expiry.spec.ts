import { test, expect } from '@playwright/test';
import { execSync } from 'node:child_process';
import { expirePendingHold } from '../setup/db-seed.js';

/**
 * Verifies the expirePendingHold seed helper against a real pending hold
 * created through booking_api.
 */
const DB_URL = process.env.DATABASE_URL || 'postgres://postgres:password@localhost:5432/our_places';
const BOOKING_API_URL = process.env.BOOKING_API_URL || 'http://127.0.0.1:8081';
const LISTING_ID = '018f3a5e-6b9c-7000-8000-000000000001';
// Dedicated guest: placing a hold cancels the guest's other pending holds,
// so this test must not share a guest with the portal suites.
const HOLD_TEST_GUEST_ID = '01a0bcbd-014d-7062-99b2-6a42d05b9f01';

function psql(sql: string): string {
  return execSync(`psql "${DB_URL}" -v ON_ERROR_STOP=1 -t -A`, { input: sql }).toString().trim();
}

test.describe('expirePendingHold', () => {
  test.beforeAll(() => {
    psql(`
      INSERT INTO "user" (id, email, password_hash, first_name, last_name, roles,
                          is_active, is_verified, default_currency, created_at, updated_at)
      VALUES ('${HOLD_TEST_GUEST_ID}', 'hold.expiry@example.com', 'x', 'Hold', 'Expiry',
              '{booker}', true, true, 'USD', NOW(), NOW())
      ON CONFLICT (email) DO NOTHING;
    `);
  });

  test('expires a pending hold the way the cleanup cron job does', async ({ request }) => {
    // A random week in 2031 avoids colliding with other bookings on the villa.
    const start = new Date(Date.UTC(2031, 0, 1 + Math.floor(Math.random() * 300)));
    const end = new Date(start.getTime() + 2 * 86_400_000);
    const day = (d: Date) => d.toISOString().slice(0, 10);

    const res = await request.post(`${BOOKING_API_URL}/api/v1/bookings`, {
      data: {
        guest_id: HOLD_TEST_GUEST_ID,
        listing_id: LISTING_ID,
        check_in: day(start),
        check_out: day(end),
        num_adults: 1,
        num_children: 0,
        num_infants: 0,
        num_pets: 0,
        message_to_host: null,
        estimated_arrival_time: null,
        is_business_trip: false,
        currency: 'USD',
        agreed_cancellation_policy: 'Flexible',
      },
    });
    expect(res.status(), await res.text()).toBe(201);
    const booking = await res.json();
    expect(psql(`SELECT status FROM booking WHERE id = '${booking.id}'`)).toBe('pending');

    expect(expirePendingHold(booking.id)).toBe(1);

    expect(psql(`SELECT status FROM booking WHERE id = '${booking.id}'`)).toBe('cancelled');
    expect(
      psql(`SELECT change_reason FROM booking_history WHERE booking_id = '${booking.id}'
            ORDER BY created_at DESC LIMIT 1`)
    ).toBe('Hold expired after 2 hours');
    // A second call is a no-op: the booking is no longer pending.
    expect(expirePendingHold(booking.id)).toBe(0);
  });

  test('rejects non-UUID input', () => {
    expect(() => expirePendingHold("x'; DROP TABLE booking; --")).toThrow(/not a UUID/);
  });
});
