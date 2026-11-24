import { test, expect } from '../fixtures/auth.fixture.js';
import { createBooking, createGuest, psql, requestDateChange } from '../fixtures/bookings-data.js';

// Parity inventory A25: change requests are handled in the booking-details
// dialog, which never opens in the legacy admin (A20). New portal only.
const NEW_ONLY = { tag: '@new-only' };

const addDays = (iso: string, n: number) =>
  new Date(Date.parse(`${iso}T00:00:00Z`) + n * 86_400_000).toISOString().slice(0, 10);

test.describe('Admin Portal - Reservation Change Requests', () => {
  test('approves a guest date-change request', NEW_ONLY, async ({ authenticatedAdminPage: page, request }) => {
    const guest = createGuest();
    const booking = await createBooking(request, guest.id, { confirmed: true });
    await requestDateChange(request, guest, booking, addDays(booking.checkIn, 1), addDays(booking.checkOut, 1));

    await page.goto(`/admin/bookings?q=${booking.confirmationCode}`);
    await page.getByRole('row', { name: new RegExp(booking.confirmationCode) }).getByRole('button', { name: /details/i }).click();
    const dialog = page.locator('#admin-booking-details-dialog');
    await expect(dialog).toBeVisible();
    await dialog.getByRole('button', { name: /approve/i }).click();

    await expect
      .poll(() => psql(`SELECT status FROM reservation_change_request WHERE booking_id = '${booking.id}';`), { timeout: 10_000 })
      .toBe('approved');
  });
});
