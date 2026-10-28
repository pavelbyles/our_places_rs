import { execSync } from 'node:child_process';
import * as crypto from 'node:crypto';
import type { APIRequestContext, Page } from '@playwright/test';
import { TEST_USERS } from './test-data.js';

const DB_URL = process.env.DATABASE_URL || 'postgres://postgres:password@localhost:5432/our_places';
const BOOKING_API_URL = process.env.BOOKING_API_URL || 'http://127.0.0.1:8081';
/** The seeded villa (db-seed.ts). */
export const SEEDED_LISTING_ID = '018f3a5e-6b9c-7000-8000-000000000001';
/** bcrypt hash of TEST_USERS.guest.password, as seeded in db-seed.ts. */
const GUEST_PASSWORD_HASH = '$2b$12$G3TzzR2meVcZAQU2OL49tel/uqz8pfao3/6jDPvWsyc9bL4EUdgk2';

export interface TestGuest {
  id: string;
  email: string;
  password: string;
}

export interface TestBooking {
  id: string;
  confirmationCode: string;
  checkIn: string;
  checkOut: string;
}

export function psql(sql: string): string {
  return execSync(`psql "${DB_URL}" -v ON_ERROR_STOP=1 -t -A`, { input: sql }).toString().trim();
}

/**
 * Creates a verified guest that only this test uses. Placing a hold cancels
 * the guest's other pending holds, so parallel tests must not share guests.
 */
export function createGuest(): TestGuest {
  const id = crypto.randomUUID();
  const email = `e2e.guest.${id.slice(0, 8)}@example.com`;
  psql(`
    INSERT INTO "user" (id, email, password_hash, first_name, last_name, roles,
                        is_active, is_verified, default_currency, created_at, updated_at)
    VALUES ('${id}', '${email}', '${GUEST_PASSWORD_HASH}', 'Casey', 'E2E',
            '{booker}', true, true, 'USD', NOW(), NOW());
  `);
  return { id, email, password: TEST_USERS.guest.password };
}

const day = (d: Date) => d.toISOString().slice(0, 10);

/**
 * Places a 2-night hold on the seeded villa through booking_api, on random
 * dates between 2032 and 2036 (retrying if they are taken).
 * With `confirmed`, the booking is then marked confirmed in the database.
 */
export async function createBooking(
  request: APIRequestContext,
  guestId: string,
  options: { confirmed?: boolean } = {},
): Promise<TestBooking> {
  for (let attempt = 0; attempt < 8; attempt++) {
    const start = new Date(Date.UTC(2032, 0, 1 + Math.floor(Math.random() * 1400)));
    const end = new Date(start.getTime() + 2 * 86_400_000);
    const res = await request.post(`${BOOKING_API_URL}/api/v1/bookings`, {
      data: {
        guest_id: guestId,
        listing_id: SEEDED_LISTING_ID,
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
    if (res.status() !== 201) continue;
    const booking = await res.json();
    if (options.confirmed) {
      psql(`UPDATE booking SET status = 'confirmed' WHERE id = '${booking.id}';`);
    }
    return {
      id: booking.id,
      confirmationCode: booking.confirmation_code,
      checkIn: day(start),
      checkOut: day(end),
    };
  }
  throw new Error('createBooking: no free dates found after 8 attempts');
}

export function bookingStatus(bookingId: string): string {
  return psql(`SELECT status FROM booking WHERE id = '${bookingId}';`);
}

export async function loginGuest(page: Page, guest: Pick<TestGuest, 'email' | 'password'>): Promise<void> {
  await page.goto('/login');
  await page.locator('#guest-email').fill(guest.email);
  await page.locator('#guest-password').fill(guest.password);
  await page.locator('#btn-guest-login').click();
  await page.waitForFunction(() => !window.location.pathname.includes('/login'), { timeout: 15000 });
}

/**
 * Signs an HS256 token in the api_core::auth::Claims format. Uses the same
 * JWT_SECRET (or local "secret" fallback) as the backends under test.
 */
export function signTestJwt(userId: string, secret = process.env.JWT_SECRET || 'secret'): string {
  const enc = (v: object) => Buffer.from(JSON.stringify(v)).toString('base64url');
  const body = `${enc({ alg: 'HS256', typ: 'JWT' })}.${enc({ sub: userId, exp: Math.floor(Date.now() / 1000) + 3600 })}`;
  return `${body}.${crypto.createHmac('sha256', secret).update(body).digest('base64url')}`;
}

/** Files a guest date-change request directly through booking_api. */
export async function requestDateChange(
  request: APIRequestContext,
  guest: TestGuest,
  booking: TestBooking,
  dateFrom: string,
  dateTo: string,
): Promise<void> {
  const res = await request.post(`${BOOKING_API_URL}/api/v1/bookings/${booking.id}/change-requests`, {
    headers: { Authorization: `Bearer ${signTestJwt(guest.id)}` },
    data: { date_from: dateFrom, date_to: dateTo, reason: 'e2e date change' },
  });
  if (!res.ok()) {
    throw new Error(`requestDateChange: ${res.status()} ${await res.text()}`);
  }
}

const LISTING_API_URL = process.env.LISTING_API_URL || 'http://127.0.0.1:8082';

/**
 * Turns a booking into a stay that ended 2 days ago (eligible for review for
 * 15 days after checkout) and returns its review token from listing_api.
 */
export async function reviewTokenForPastStay(
  request: APIRequestContext,
  guest: TestGuest,
  booking: TestBooking,
): Promise<string> {
  psql(`UPDATE booking
        SET status = 'completed', date_from = CURRENT_DATE - 5, date_to = CURRENT_DATE - 2
        WHERE id = '${booking.id}';`);
  const res = await request.get(`${LISTING_API_URL}/api/v1/reviews/booking/${booking.id}/token`, {
    headers: { Authorization: `Bearer ${signTestJwt(guest.id)}` },
  });
  const body = await res.json();
  if (!res.ok() || !body.token) {
    throw new Error(`reviewTokenForPastStay: ${res.status()} ${JSON.stringify(body)}`);
  }
  return body.token as string;
}
