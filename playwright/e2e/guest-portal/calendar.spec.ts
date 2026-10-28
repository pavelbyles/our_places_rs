import { test, expect } from '../fixtures/base.fixture.js';
import { createBooking, createGuest } from '../fixtures/bookings-data.js';
import { KNOWN_LISTINGS } from '../fixtures/test-data.js';

// Parity inventory G7 (decision D3/D4): the stay-dates calendar is new in
// the redesigned portal. Behaviour details are covered by e2e/ui-js.
const NEW_ONLY = { tag: '@new-only' };
const villa = KNOWN_LISTINGS.courtyardStudio;

test.describe('Guest Portal - Stay Dates Calendar', () => {
  test('greys out booked nights on the listing calendar', NEW_ONLY, async ({ page, request }) => {
    const booking = await createBooking(request, createGuest().id, { confirmed: true });
    const month = booking.checkIn.slice(0, 7);

    await page.goto(`/listings/${villa.slug}/calendar?month=${month}`);
    const firstNight = page.locator(`button[data-date="${booking.checkIn}"]`);
    await expect(firstNight).toHaveAttribute('data-state', 'booked');
    await expect(firstNight).toHaveAttribute('aria-disabled', 'true');
    await expect(page.locator(`button[data-date="${booking.checkOut}"]`)).not.toHaveAttribute('data-state', 'past');
  });

  test('selects dates with the keyboard and refreshes the quote', NEW_ONLY, async ({ page }) => {
    await page.goto(`/listings/${villa.slug}`);
    const calendar = page.locator('#booking-dates');
    await expect(calendar).toHaveAttribute('data-enhanced', /.*/);

    const start = calendar.locator('button[data-date][data-state="available"]').first();
    await start.focus();
    await page.keyboard.press('Enter');
    for (let i = 0; i < 3; i++) await page.keyboard.press('ArrowRight');
    await page.keyboard.press('Enter');

    await expect(page.locator('input[name="check_out"]')).not.toHaveValue('');
    await expect(page.locator('#breakdown-total')).toBeVisible();
  });

  test('moves to the next month over HTMX', NEW_ONLY, async ({ page }) => {
    await page.goto(`/listings/${villa.slug}`);
    const firstCaption = await page.locator('#booking-dates caption').first().textContent();
    await page.getByRole('button', { name: 'Next month' }).click();
    await expect(page.locator('#booking-dates caption').first()).not.toHaveText(firstCaption ?? '');
    await expect(page.locator('#booking-dates')).toHaveAttribute('data-enhanced', /.*/);
  });
});
