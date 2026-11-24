import { test, expect } from '../fixtures/auth.fixture.js';
import { createBooking, createGuest } from '../fixtures/bookings-data.js';

test.describe('Admin Portal - Booking Messages', () => {
  test('replies to a guest thread', async ({ authenticatedAdminPage: page, request }) => {
    const booking = await createBooking(request, createGuest().id, { confirmed: true });

    const resp = await page.goto(`/admin/bookings/${booking.confirmationCode}/messages`);
    expect(resp?.status()).toBe(200);
    const text = `Your door code is ready ${Date.now()}`;
    await page.locator('[name="message_text"]').fill(text);
    await page.locator('form:has([name="message_text"]) [type="submit"]').click();
    await expect(page.locator('#admin-messages-list')).toContainText(text);
  });
});
