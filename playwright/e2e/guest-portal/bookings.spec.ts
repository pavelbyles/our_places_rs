import { test, expect } from '../fixtures/base.fixture.js';
import { bookingStatus, createBooking, createGuest, loginGuest } from '../fixtures/bookings-data.js';
import { expirePendingHold } from '../setup/db-seed.js';

// Legacy deviations (parity inventory): the legacy page's inline script is
// HTML-escaped by Topcoat ("SyntaxError: Unexpected token ';'"), so its cancel
// dialog never opens (G15); it also lists every booking on the platform (G14)
// and its refund quote never loads (G16). These tests run on the new portal.
const NEW_ONLY = { tag: '@new-only' };

test.describe('Guest Portal - My Bookings', () => {
  test('lists the guest booking with its actions', async ({ page, request }) => {
    const guest = createGuest();
    const booking = await createBooking(request, guest.id, { confirmed: true });
    await loginGuest(page, guest);

    const resp = await page.goto('/bookings');
    expect(resp?.status()).toBe(200);
    const container = page.locator('#upcoming-bookings-container');
    await expect(container).toContainText(booking.confirmationCode);
    await expect(page.locator(`#btn-cancel-${booking.id}`)).toBeVisible();
    await expect(page.locator(`#btn-change-dates-${booking.id}`)).toHaveAttribute(
      'href',
      `/bookings/${booking.id}/change-dates`,
    );
  });

  test('cancels a booking after confirming in the dialog', NEW_ONLY, async ({ page, request }) => {
    const guest = createGuest();
    const booking = await createBooking(request, guest.id, { confirmed: true });
    await loginGuest(page, guest);
    await page.goto('/bookings');

    await page.locator(`#btn-cancel-${booking.id}`).click();
    const dialog = page.locator('#cancel-booking-modal');
    await expect(dialog).toBeVisible();
    await dialog.getByRole('button', { name: 'Yes, Cancel Booking' }).click();

    await expect.poll(() => bookingStatus(booking.id), { timeout: 10_000 }).not.toBe('confirmed');
    await page.reload();
    await expect(page.locator(`#btn-cancel-${booking.id}`)).toHaveCount(0);
  });

  test('redirects a signed-out visitor to login', async ({ page }) => {
    await page.goto('/bookings');
    await expect(page).toHaveURL(/\/login/);
  });

  test('never lists another guest booking', NEW_ONLY, async ({ page, request }) => {
    const owner = createGuest();
    const other = await createBooking(request, createGuest().id, { confirmed: true });
    await createBooking(request, owner.id, { confirmed: true });
    await loginGuest(page, owner);

    await page.goto('/bookings');
    await expect(page.locator('#upcoming-bookings-container')).not.toContainText(other.confirmationCode);
    await expect(page.locator(`#btn-cancel-${other.id}`)).toHaveCount(0);
  });

  test('shows the refund quote before cancelling', NEW_ONLY, async ({ page, request }) => {
    const guest = createGuest();
    const booking = await createBooking(request, guest.id, { confirmed: true });
    await loginGuest(page, guest);
    await page.goto('/bookings');

    await page.locator(`#btn-cancel-${booking.id}`).click();
    await expect(page.locator('#cancel-modal-quote-details')).toContainText('Refund Total');
  });

  test('hides another guest booking quote', NEW_ONLY, async ({ page, request }) => {
    const other = await createBooking(request, createGuest().id, { confirmed: true });
    await loginGuest(page, createGuest());

    const resp = await page.goto(`/bookings/${other.id}/cancellation-quote`);
    expect(resp?.status()).toBe(404);
  });

  // Hold expiry (intent §5, 2-hour hold): once the cleanup job cancels a
  // pending hold, the guest is offered the same dates again.
  test('offers to book again after a hold expires', NEW_ONLY, async ({ page, request }) => {
    const guest = createGuest();
    const booking = await createBooking(request, guest.id);
    await loginGuest(page, guest);
    await page.goto('/bookings');
    await expect(page.locator(`#booking-${booking.id}`)).toContainText('held until');

    expect(expirePendingHold(booking.id)).toBe(1);
    await page.reload();
    const card = page.locator(`#booking-${booking.id}`);
    await expect(page.locator(`#status-badge-${booking.id}`)).toHaveText(/cancelled/i);
    await expect(card.getByRole('link', { name: 'Book again' })).toHaveAttribute(
      'href',
      new RegExp(`check_in=${booking.checkIn}&check_out=${booking.checkOut}`),
    );
    await expect(page.locator(`#btn-cancel-${booking.id}`)).toHaveCount(0);
  });
});

