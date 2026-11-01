import { test, expect } from '../fixtures/base.fixture.js';
import { createBooking, createGuest, loginGuest, psql, requestDateChange } from '../fixtures/bookings-data.js';

// Legacy deviations (parity inventory G18-G23): Topcoat HTML-escapes the
// legacy pages' inline scripts, so they never run. The change-dates form
// throws "SyntaxError: Unexpected token '&'" and the My Bookings page throws
// "Unexpected token ';'" (its withdraw/approve/decline buttons do nothing).
// The page itself renders on both portals (tax-documents.spec.ts).
const NEW_ONLY = { tag: '@new-only' };

const addDays = (iso: string, n: number) =>
  new Date(Date.parse(`${iso}T00:00:00Z`) + n * 86_400_000).toISOString().slice(0, 10);

const pendingRequests = (bookingId: string) =>
  psql(`SELECT count(*) FROM reservation_change_request WHERE booking_id = '${bookingId}' AND status = 'pending';`);

test.describe('Guest Portal - Reservation Changes', () => {
  test('shows a pending request on My Bookings and withdraws it', NEW_ONLY, async ({ page, request }) => {
    const guest = createGuest();
    const booking = await createBooking(request, guest.id, { confirmed: true });
    await requestDateChange(request, guest, booking, addDays(booking.checkIn, 1), addDays(booking.checkOut, 1));
    await loginGuest(page, guest);

    await page.goto('/bookings');
    const banner = page.locator(`#pending-change-banner-${booking.id}`);
    await expect(banner).toBeVisible();
    await banner.locator('button[id^="btn-cancel-change-"]').click();

    await expect.poll(() => pendingRequests(booking.id), { timeout: 10_000 }).toBe('0');
    await expect(page.locator(`#pending-change-banner-${booking.id}`)).toHaveCount(0, { timeout: 10_000 });
  });

  test('requests new dates from the change-dates page', NEW_ONLY, async ({ page, request }) => {
    const guest = createGuest();
    const booking = await createBooking(request, guest.id, { confirmed: true });
    await loginGuest(page, guest);

    await page.goto(`/bookings/${booking.id}/change-dates`);
    await page.locator('#new-check-in').fill(addDays(booking.checkIn, 1));
    await page.locator('#new-check-out').fill(addDays(booking.checkOut, 1));
    await page.locator('#btn-preview-change-quote').click();
    await expect(page.locator('#change-quote-preview')).toBeVisible();
    await expect(page.locator('#date-change-error')).toBeHidden();

    await page.locator('#btn-submit-date-change').click();
    await expect(page).toHaveURL(/\/bookings$/, { timeout: 10_000 });
    await expect(page.locator(`#pending-change-banner-${booking.id}`)).toBeVisible();
    expect(pendingRequests(booking.id)).toBe('1');
  });

  test('rejects a check-out before the check-in', NEW_ONLY, async ({ page, request }) => {
    const guest = createGuest();
    const booking = await createBooking(request, guest.id, { confirmed: true });
    await loginGuest(page, guest);

    await page.goto(`/bookings/${booking.id}/change-dates`);
    await page.locator('#new-check-in').fill(addDays(booking.checkIn, 3));
    await page.locator('#new-check-out').fill(addDays(booking.checkIn, 1));
    await page.locator('#btn-submit-date-change').click();
    await expect(page.locator('#date-change-error')).toBeVisible();
  });
});
