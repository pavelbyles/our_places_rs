import { test, expect } from '@playwright/test';
import * as crypto from 'node:crypto';

/**
 * Generates an HS256 JWT bearer token matching api_core::auth::Claims format.
 */
function generateTestJwt(userId: string, secret = process.env.JWT_SECRET || 'secret'): string {
  const header = Buffer.from(JSON.stringify({ alg: 'HS256', typ: 'JWT' })).toString('base64url');
  const payload = Buffer.from(
    JSON.stringify({
      sub: userId,
      exp: Math.floor(Date.now() / 1000) + 3600,
    })
  ).toString('base64url');
  const signature = crypto
    .createHmac('sha256', secret)
    .update(`${header}.${payload}`)
    .digest('base64url');
  return `${header}.${payload}.${signature}`;
}

test.describe('Backend API Validation - Transactional Messaging & Sweeps', () => {
  const bookingApiUrl = process.env.BOOKING_API_URL || 'http://127.0.0.1:8081';
  const userApiUrl = process.env.USER_API_URL || 'http://127.0.0.1:8083';
  const cronSecret = process.env.CRON_SECRET || 'dev-cron-secret';

  // Seeded user IDs matching db-seed.ts
  let hostUserId = '01a0bcbd-014d-7062-99b2-6a42d05b9ed7';
  const guestUserId = '01a0bcbd-014d-7062-99b2-6a42d05b9ee8';
  const testBookingId = '018f3a5e-6b9c-7000-8000-000000000010';

  test.beforeAll(async ({ request }) => {
    try {
      const loginRes = await request.post(`${userApiUrl}/api/v1/users/login`, {
        data: {
          email: 'admin@ourplaces.io',
          password: 'admin_changeme_2026',
        },
      });
      if (loginRes.ok()) {
        const user = await loginRes.json();
        if (user.id) {
          hostUserId = user.id;
        }
      }
    } catch {
      // Fallback to static hostUserId
    }
  });

  test.describe('POST /api/v1/internal/cron/process-scheduled-notifications', () => {
    test('should reject request missing x-cron-secret header with 401 Unauthorized', async ({ request }) => {
      const response = await request.post(`${bookingApiUrl}/api/v1/internal/cron/process-scheduled-notifications`);
      expect(response.status()).toBe(401);
    });

    test('should reject request with invalid x-cron-secret header with 401 Unauthorized', async ({ request }) => {
      const response = await request.post(`${bookingApiUrl}/api/v1/internal/cron/process-scheduled-notifications`, {
        headers: {
          'x-cron-secret': 'invalid-secret-key-123',
        },
      });
      expect(response.status()).toBe(401);
    });

    test('should accept valid x-cron-secret and execute sweep with 200 OK', async ({ request }) => {
      const response = await request.post(`${bookingApiUrl}/api/v1/internal/cron/process-scheduled-notifications`, {
        headers: {
          'x-cron-secret': cronSecret,
        },
      });
      expect(response.status()).toBe(200);

      const body = await response.json();
      expect(body.status).toBe('success');
      expect(typeof (body.pre_arrival_processed ?? body.pre_arrival_notifications_dispatched)).toBe('number');
      expect(typeof (body.hold_reminders_processed ?? body.expiring_hold_notifications_dispatched)).toBe('number');
    });

    test('should ensure idempotent execution on immediate successive sweep invocations', async ({ request }) => {
      // First run processes candidates
      const res1 = await request.post(`${bookingApiUrl}/api/v1/internal/cron/process-scheduled-notifications`, {
        headers: { 'x-cron-secret': cronSecret },
      });
      expect(res1.status()).toBe(200);

      // Second immediate run should find 0 new candidates due to booking_notification_log unique constraint
      const res2 = await request.post(`${bookingApiUrl}/api/v1/internal/cron/process-scheduled-notifications`, {
        headers: { 'x-cron-secret': cronSecret },
      });
      expect(res2.status()).toBe(200);

      const body2 = await res2.json();
      expect(body2.status).toBe('success');
      expect(body2.pre_arrival_processed ?? body2.pre_arrival_notifications_dispatched).toBe(0);
      expect(body2.hold_reminders_processed ?? body2.expiring_hold_notifications_dispatched).toBe(0);
    });
  });

  test.describe('PATCH /api/v1/bookings/:id - Door Access Code Authorization Guard', () => {
    test('should reject door_access_code modification without authorization header with 401 Unauthorized', async ({ request }) => {
      const response = await request.patch(`${bookingApiUrl}/api/v1/bookings/${testBookingId}`, {
        data: {
          door_access_code: 'UNAUTH-9999',
        },
      });
      expect(response.status()).toBe(401);
    });

    test('should reject door_access_code modification by guest user with 403 Forbidden', async ({ request }) => {
      const guestToken = generateTestJwt(guestUserId);
      const response = await request.patch(`${bookingApiUrl}/api/v1/bookings/${testBookingId}`, {
        headers: {
          Authorization: `Bearer ${guestToken}`,
        },
        data: {
          door_access_code: 'GUEST-HACK-1234',
        },
      });
      expect(response.status()).toBe(403);
    });

    test('should allow door_access_code modification by host/admin with 200 OK', async ({ request }) => {
      const hostToken = generateTestJwt(hostUserId);
      const testCode = 'HOST-KEY-7788';
      const response = await request.patch(`${bookingApiUrl}/api/v1/bookings/${testBookingId}`, {
        headers: {
          Authorization: `Bearer ${hostToken}`,
        },
        data: {
          door_access_code: testCode,
        },
      });
      expect(response.status()).toBe(200);

      const body = await response.json();
      expect(body.id).toBe(testBookingId);
      expect(body.door_access_code).toBe(testCode);
    });
  });

  test.describe('PATCH /api/v1/bookings/:id - Material Change & Status Transition Hooks', () => {
    test('should allow host to execute material booking update (modifying guest count and notes)', async ({ request }) => {
      const hostToken = generateTestJwt(hostUserId);
      const response = await request.patch(`${bookingApiUrl}/api/v1/bookings/${testBookingId}`, {
        headers: {
          Authorization: `Bearer ${hostToken}`,
        },
        data: {
          metadata: {
            num_adults: 3,
            num_children: 1,
            num_infants: 0,
            num_pets: 0,
            is_business_trip: false,
            message_to_host: 'Late checkout requested at 1:00 PM',
          },
        },
      });
      expect(response.status()).toBe(200);

      const body = await response.json();
      expect(body.id).toBe(testBookingId);
      expect(body.metadata?.num_adults).toBe(3);
      expect(body.metadata?.num_children).toBe(1);
    });

    test('should allow host/admin to cancel booking and trigger cancellation status', async ({ request }) => {
      const hostToken = generateTestJwt(hostUserId);
      const response = await request.patch(`${bookingApiUrl}/api/v1/bookings/${testBookingId}`, {
        headers: {
          Authorization: `Bearer ${hostToken}`,
        },
        data: {
          status: 'cancelled',
        },
      });
      expect(response.status()).toBe(200);

      const body = await response.json();
      expect(body.id).toBe(testBookingId);
      expect(body.status.toLowerCase()).toBe('cancelled');

      // Restore confirmed status for other test runs
      await request.patch(`${bookingApiUrl}/api/v1/bookings/${testBookingId}`, {
        headers: {
          Authorization: `Bearer ${hostToken}`,
        },
        data: {
          status: 'confirmed',
        },
      });
    });
  });
});
