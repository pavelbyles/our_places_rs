import { randomInt } from 'node:crypto';
import type { Browser, Page } from '@playwright/test';
import { createHost, type TestHost } from './admin-data.js';
import { createListingCopy, psql } from './bookings-data.js';
import { TEST_CONFIG } from './test-data.js';

export type BookingMode = 'instant' | 'request' | 'both';

/** A copy of the seeded villa in `mode`, owned by a fresh host. */
export function listingInMode(mode: BookingMode): { id: string; slug: string; name: string; host: TestHost } {
  const host = createHost();
  const copy = createListingCopy();
  psql(`UPDATE listing SET booking_mode = '${mode}', user_id = '${host.id}' WHERE id = '${copy.id}';`);
  return { ...copy, host };
}

/** A random 3-night stay far enough ahead to be free of other bookings. */
export function farStay(): { checkIn: string; checkOut: string } {
  const start = new Date(Date.UTC(2039, 0, 1 + randomInt(1500)));
  const end = new Date(start.getTime() + 3 * 86_400_000);
  return { checkIn: start.toISOString().slice(0, 10), checkOut: end.toISOString().slice(0, 10) };
}

/** A signed-in admin-portal page for `host`, in its own browser context. */
export async function hostPage(browser: Browser, host: TestHost): Promise<Page> {
  const context = await browser.newContext({ baseURL: TEST_CONFIG.adminPortalUrl });
  const page = await context.newPage();
  await page.goto('/login');
  await page.locator('#admin-login-email').fill(host.email);
  await page.locator('#admin-login-password').fill(host.password);
  await page.locator('#btn-admin-login').click();
  await page.waitForFunction(() => !window.location.pathname.includes('/login'), { timeout: 15000 });
  return page;
}

/** Submits a request to book for `slug` from checkout; returns its id. */
export async function submitRequest(page: Page, slug: string, stay: { checkIn: string; checkOut: string }, message = ''): Promise<string> {
  await page.goto(`/checkout/${slug}?check_in=${stay.checkIn}&check_out=${stay.checkOut}&guests=2`);
  if (message) {
    await page.locator('#guest-message').fill(message);
  }
  await page.locator('#checkout-request-btn').click();
  await page.waitForURL(/\/requests\/[0-9a-f-]{36}$/);
  return page.url().split('/').pop() ?? '';
}

export function requestStatus(id: string): string {
  return psql(`SELECT status FROM booking_request WHERE id = '${id}';`);
}
