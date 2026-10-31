import { test, expect } from '../fixtures/base.fixture.js';
import { createBooking, createGuest, loginGuest } from '../fixtures/bookings-data.js';

// Parity inventory G24: the legacy page finds the booking among all bookings,
// so any signed-in guest can open any thread.
const NEW_ONLY = { tag: '@new-only' };

test.describe('Guest Portal - Guest-Host Messaging', () => {
  test('sends a message to the host', async ({ page, request }) => {
    const guest = createGuest();
    const booking = await createBooking(request, guest.id, { confirmed: true });
    await loginGuest(page, guest);

    const resp = await page.goto(`/bookings/${booking.confirmationCode}/messages`);
    expect(resp?.status()).toBe(200);
    const text = `Arriving around 4pm ${Date.now()}`;
    await page.locator('[name="message_text"]').fill(text);
    await page.locator('form:has([name="message_text"]) button[type="submit"]').click();
    await expect(page.locator('#messages-list')).toContainText(text);
  });

  test('redirects a signed-out visitor to login', async ({ page, request }) => {
    const booking = await createBooking(request, createGuest().id, { confirmed: true });
    await page.goto(`/bookings/${booking.confirmationCode}/messages`);
    await expect(page).toHaveURL(/\/login/);
  });

  test('hides another guest thread', NEW_ONLY, async ({ page, request }) => {
    const other = await createBooking(request, createGuest().id, { confirmed: true });
    await loginGuest(page, createGuest());

    const resp = await page.goto(`/bookings/${other.confirmationCode}/messages`);
    expect(resp?.status()).toBe(404);
  });
});
