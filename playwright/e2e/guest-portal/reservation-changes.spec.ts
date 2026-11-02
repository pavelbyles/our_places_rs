import { test, expect } from '../fixtures/base.fixture.js';
import { createBooking, createGuest, createPromoCode, loginGuest, psql, requestDateChange } from '../fixtures/bookings-data.js';

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

  /** Attaches a 10% code needing a 2-night stay to a confirmed 2-night booking. */
  function attachPromo(bookingId: string, guestId: string): string {
    const code = createPromoCode(guestId, 'min_nights = 2');
    const terms = JSON.stringify({
      code,
      discount_type: 'percentage',
      discount_value: '10',
      scope: 'stay',
      currency: null,
      exchange_rate: '1',
      min_nights: 2,
      min_booking_value: null,
      stay_from: null,
      stay_to: null,
    });
    psql(`UPDATE booking SET promo_code_id = (SELECT id FROM promo_code WHERE code = '${code}'),
            promo_terms = '${terms}', promo_stay_discount = 130 WHERE id = '${bookingId}';
          INSERT INTO promo_code_redemption (promo_code_id, booking_id)
            SELECT id, '${bookingId}' FROM promo_code WHERE code = '${code}';`);
    return code;
  }

  test('the change page warns before a change loses the discount', NEW_ONLY, async ({ page, request }) => {
    const guest = createGuest();
    const booking = await createBooking(request, guest.id, { confirmed: true });
    attachPromo(booking.id, guest.id);
    await loginGuest(page, guest);
    await page.goto(`/bookings/${booking.id}/change-dates`);

    // One night is below the code's minimum.
    await page.locator('#new-check-in').fill(booking.checkIn);
    await page.locator('#new-check-out').fill(addDays(booking.checkIn, 1));
    await page.locator('#btn-preview-change-quote').click();
    await expect(page.locator('#change-promo-lost')).toContainText('removes your promo discount');
    await expect(page.locator('#change-promo-lost')).toContainText('too short');

    // A longer stay keeps it.
    await page.locator('#new-check-out').fill(addDays(booking.checkOut, 1));
    await page.locator('#btn-preview-change-quote').click();
    await expect(page.locator('#change-quote-preview')).toContainText('New total');
    await expect(page.locator('#change-promo-lost')).toHaveCount(0);
  });

  test('a booking with no promo gets no warning', NEW_ONLY, async ({ page, request }) => {
    const guest = createGuest();
    const booking = await createBooking(request, guest.id, { confirmed: true });
    await loginGuest(page, guest);
    await page.goto(`/bookings/${booking.id}/change-dates`);

    await page.locator('#new-check-in').fill(booking.checkIn);
    await page.locator('#new-check-out').fill(addDays(booking.checkIn, 1));
    await page.locator('#btn-preview-change-quote').click();
    await expect(page.locator('#change-quote-preview')).toContainText('New total');
    await expect(page.locator('#change-promo-lost')).toHaveCount(0);
  });

  test('a host-proposed change shows the lost discount above Accept', NEW_ONLY, async ({ page, request }) => {
    const guest = createGuest();
    const booking = await createBooking(request, guest.id, { confirmed: true });
    await requestDateChange(request, guest, booking, addDays(booking.checkIn, 1), addDays(booking.checkOut, 1));
    psql(`UPDATE reservation_change_request
          SET initiated_by_role = 'host',
              pricing_snapshot = pricing_snapshot || '{"promo_lost_reason":"This stay is too short for that promo code."}'
          WHERE booking_id = '${booking.id}';`);
    await loginGuest(page, guest);

    await page.goto('/bookings');
    await expect(page.locator(`#pending-change-promo-lost-${booking.id}`)).toContainText('too short');
    await expect(page.locator(`#pending-change-banner-${booking.id}`).getByRole('button', { name: 'Accept change' })).toBeVisible();
  });
});

