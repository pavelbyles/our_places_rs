import { test, expect } from '../fixtures/base.fixture.js';
import { KNOWN_LISTINGS, TEST_USERS } from '../fixtures/test-data.js';
import { createGuest, loginGuest, psql } from '../fixtures/bookings-data.js';

// Fixed deviations from the retired legacy portal (parity inventory G8/G13,
// decisions D1/D2) stay tagged @new-only.
const NEW_ONLY = { tag: '@new-only' };

test.describe('Guest Portal - Checkout Flow & 15-Minute Hold', () => {
  const targetVilla = KNOWN_LISTINGS.courtyardStudio;

  /** A random 5-night stay far enough ahead to be free of other bookings. */
  function freeStay(): { checkIn: string; checkOut: string } {
    const start = new Date(Date.UTC(2038, 0, 1 + Math.floor(Math.random() * 1000)));
    const end = new Date(start.getTime() + 5 * 86_400_000);
    return { checkIn: start.toISOString().slice(0, 10), checkOut: end.toISOString().slice(0, 10) };
  }

  test('sends a signed-out guest to sign in and back with the same stay', NEW_ONLY, async ({ page }) => {
    const guest = createGuest();
    const { checkIn, checkOut } = freeStay();
    await page.goto(`/checkout/${targetVilla.slug}?check_in=${checkIn}&check_out=${checkOut}&guests=2`);
    await expect(page).toHaveURL(/\/login\?redirect=/);

    await page.locator('#guest-email').fill(guest.email);
    await page.locator('#guest-password').fill(guest.password);
    await page.locator('#btn-guest-login').click();

    await expect(page).toHaveURL(new RegExp(`/checkout/${targetVilla.slug}\\?check_in=${checkIn}`));
    await expect(page.locator('#breakdown-subtotal')).toContainText('3250.00');
    await expect(page.locator('#breakdown-cleaning-fee')).toContainText('75.00');
    await expect(page.locator('#breakdown-tax')).toContainText('498.75');
    await expect(page.locator('#breakdown-damage-deposit')).toContainText('150.00');
    await expect(page.locator('#breakdown-total')).toContainText('3973.75');
    await expect(page.locator('#guest-email')).toHaveValue(guest.email);
  });

  test('places a hold for the signed-in guest on the chosen dates', NEW_ONLY, async ({ page }) => {
    const guest = createGuest();
    const { checkIn, checkOut } = freeStay();
    await loginGuest(page, guest);
    await page.goto(`/checkout/${targetVilla.slug}?check_in=${checkIn}&check_out=${checkOut}&guests=2`);

    await page.locator('#guest-phone').fill(TEST_USERS.guest.phone);
    await page.locator('#checkout-submit-btn').click();
    await expect(page).toHaveURL(/\/bookings\?new_booking=true$/);

    const row = psql(`SELECT guest_id, date_from, date_to, status FROM booking
                      WHERE guest_id = '${guest.id}' ORDER BY created_at DESC LIMIT 1;`);
    expect(row).toBe(`${guest.id}|${checkIn}|${checkOut}|pending`);
  });

  test('rejects more guests than the listing allows', NEW_ONLY, async ({ page }) => {
    const { checkIn, checkOut } = freeStay();
    await loginGuest(page, createGuest());
    await page.goto(`/checkout/${targetVilla.slug}?check_in=${checkIn}&check_out=${checkOut}&guests=2`);
    await page.locator('input[name="guests"]').evaluate((el: HTMLInputElement) => { el.value = '9'; });
    await page.locator('#checkout-submit-btn').click();
    await expect(page.locator('#checkout-error-banner')).toContainText('guests');
  });

  // Intent §5: if the session expires mid-checkout the guest signs in again
  // and resumes; no hold is created or extended in between.
  test('resumes checkout after the session expires', NEW_ONLY, async ({ page, context }) => {
    const guest = createGuest();
    const { checkIn, checkOut } = freeStay();
    await loginGuest(page, guest);
    await page.goto(`/checkout/${targetVilla.slug}?check_in=${checkIn}&check_out=${checkOut}&guests=2`);

    await context.clearCookies();
    await page.locator('#checkout-submit-btn').click();
    await expect(page).toHaveURL(/\/login\?redirect=/);
    expect(psql(`SELECT count(*) FROM booking WHERE guest_id = '${guest.id}';`)).toBe('0');

    await page.locator('#guest-email').fill(guest.email);
    await page.locator('#guest-password').fill(guest.password);
    await page.locator('#btn-guest-login').click();
    await expect(page.locator('input[name="check_in"]')).toHaveValue(checkIn);
    await page.locator('#checkout-submit-btn').click();
    await expect(page).toHaveURL(/\/bookings\?new_booking=true$/);
    expect(psql(`SELECT count(*) FROM booking WHERE guest_id = '${guest.id}' AND status = 'pending';`)).toBe('1');
  });

  test('redirects legacy per-currency checkout URLs', NEW_ONLY, async ({ page }) => {
    await loginGuest(page, createGuest());
    await page.goto(`/checkout-jmd/${targetVilla.slug}`);
    await expect(page).toHaveURL(new RegExp(`/checkout/${targetVilla.slug}\\?currency=JMD`));
  });
});

