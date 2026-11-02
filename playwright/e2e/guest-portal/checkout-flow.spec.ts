import { randomInt } from 'node:crypto';
import type { Page } from '@playwright/test';
import { test, expect } from '../fixtures/base.fixture.js';
import { KNOWN_LISTINGS, TEST_USERS } from '../fixtures/test-data.js';
import { createGuest, createPromoCode, loginGuest, psql } from '../fixtures/bookings-data.js';

// Fixed deviations from the retired legacy portal (parity inventory G8/G13,
// decisions D1/D2) stay tagged @new-only.
const NEW_ONLY = { tag: '@new-only' };

test.describe('Guest Portal - Checkout Flow & Date Hold', () => {
  const targetVilla = KNOWN_LISTINGS.courtyardStudio;

  /** A random 5-night stay far enough ahead to be free of other bookings. */
  function freeStay(): { checkIn: string; checkOut: string } {
    const start = new Date(Date.UTC(2038, 0, 1 + randomInt(1000)));
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
    await expect(page.locator('#breakdown-platform-fee')).toContainText('162.50');
    await expect(page.locator('#breakdown-total')).toContainText('4136.25');
    await expect(page.locator('#guest-email')).toHaveValue(guest.email);
  });

  test('the listing page, checkout and hold all show the same total', NEW_ONLY, async ({ page }) => {
    const guest = createGuest();
    const { checkIn, checkOut } = freeStay();
    await loginGuest(page, guest);

    await page.goto(`/listings/${targetVilla.slug}?check_in=${checkIn}&check_out=${checkOut}&guests=2`);
    const listingTotal = await page.locator('#breakdown-total').innerText();

    await page.goto(`/checkout/${targetVilla.slug}?check_in=${checkIn}&check_out=${checkOut}&guests=2`);
    await expect(page.locator('#breakdown-total')).toHaveText(listingTotal);

    await page.locator('#guest-phone').fill(TEST_USERS.guest.phone);
    await page.locator('#checkout-submit-btn').click();
    await expect(page).toHaveURL(/\/bookings\?new_booking=true$/);
    const stored = psql(`SELECT total_price FROM booking WHERE guest_id = '${guest.id}' ORDER BY created_at DESC LIMIT 1;`);
    expect(listingTotal).toContain(stored);
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

  test.describe('promo codes', () => {
    const openCheckout = async (page: Page) => {
      const guest = createGuest();
      const { checkIn, checkOut } = freeStay();
      await loginGuest(page, guest);
      await page.goto(`/checkout/${targetVilla.slug}?check_in=${checkIn}&check_out=${checkOut}&guests=2`);
      return guest;
    };

    test('applying a code shows the discount and confirming stores it', NEW_ONLY, async ({ page }) => {
      const guest = await openCheckout(page);
      const code = createPromoCode(guest.id);
      const fullTotal = await page.locator('#breakdown-total').innerText();

      await page.locator('#checkout-promo-code').fill(code.toLowerCase());
      await page.locator('#checkout-promo-apply').click();

      await expect(page.locator('#breakdown-promo')).toContainText('325.00');
      await expect(page.locator('#breakdown-total')).not.toHaveText(fullTotal);
      const discountedTotal = await page.locator('#breakdown-total').innerText();

      await page.locator('#guest-phone').fill(TEST_USERS.guest.phone);
      await page.locator('#checkout-submit-btn').click();
      await expect(page).toHaveURL(/\/bookings\?new_booking=true$/);

      const row = psql(`SELECT total_price, promo_stay_discount, promo_terms->>'code' FROM booking
                        WHERE guest_id = '${guest.id}' ORDER BY created_at DESC LIMIT 1;`);
      const [total, discount, stored] = row.split('|');
      expect(discountedTotal).toContain(total);
      expect(discount).toBe('325.00');
      expect(stored).toBe(code);
    });

    test('an expired code shows its reason and the full price', NEW_ONLY, async ({ page }) => {
      const guest = await openCheckout(page);
      const code = createPromoCode(guest.id, "valid_to = NOW() - INTERVAL '1 day'");
      const fullTotal = await page.locator('#breakdown-total').innerText();

      await page.locator('#checkout-promo-code').fill(code);
      await page.locator('#checkout-promo-apply').click();

      await expect(page.locator('#checkout-promo-refusal')).toContainText('expired');
      await expect(page.locator('#breakdown-promo')).toHaveCount(0);
      await expect(page.locator('#breakdown-total')).toHaveText(fullTotal);
    });

    test('remove clears the code and restores the full price', NEW_ONLY, async ({ page }) => {
      const guest = await openCheckout(page);
      const code = createPromoCode(guest.id);
      const fullTotal = await page.locator('#breakdown-total').innerText();

      await page.locator('#checkout-promo-code').fill(code);
      await page.locator('#checkout-promo-apply').click();
      await expect(page.locator('#breakdown-promo')).toBeVisible();

      await page.locator('#checkout-promo-remove').click();
      await expect(page.locator('#checkout-promo-code')).toHaveValue('');
      await expect(page.locator('#breakdown-promo')).toHaveCount(0);
      await expect(page.locator('#breakdown-total')).toHaveText(fullTotal);
    });

    test('details the guest entered are kept after applying', NEW_ONLY, async ({ page }) => {
      const guest = await openCheckout(page);
      const code = createPromoCode(guest.id);
      await page.locator('#guest-phone').fill('+1 876 555 0100');
      await page.locator('#guest-message').fill('Arriving with a pram');

      await page.locator('#checkout-promo-code').fill(code);
      await page.locator('#checkout-promo-apply').click();

      await expect(page.locator('#breakdown-promo')).toBeVisible();
      await expect(page.locator('#guest-phone')).toHaveValue('+1 876 555 0100');
      await expect(page.locator('#guest-message')).toHaveValue('Arriving with a pram');
      await expect(page.locator('#checkout-promo-code')).toHaveValue(code);
    });

    test('pressing Enter applies the code and places no booking', NEW_ONLY, async ({ page }) => {
      const guest = await openCheckout(page);
      const code = createPromoCode(guest.id);

      await page.locator('#checkout-promo-code').fill(code);
      await page.locator('#checkout-promo-code').press('Enter');

      await expect(page.locator('#breakdown-promo')).toBeVisible();
      expect(psql(`SELECT COUNT(*) FROM booking WHERE guest_id = '${guest.id}';`)).toBe('0');
    });

    test('a code invalidated before confirming shows the reason and keeps the code', NEW_ONLY, async ({ page }) => {
      const guest = await openCheckout(page);
      const code = createPromoCode(guest.id);
      await page.locator('#checkout-promo-code').fill(code);
      await page.locator('#checkout-promo-apply').click();
      await expect(page.locator('#breakdown-promo')).toBeVisible();

      psql(`UPDATE promo_code SET deactivated_at = NOW(), deactivated_by = created_by WHERE code = '${code}';`);
      await page.locator('#guest-phone').fill(TEST_USERS.guest.phone);
      await page.locator('#checkout-submit-btn').click();

      await expect(page.locator('#checkout-error-banner')).toContainText('no longer active');
      await expect(page.locator('#checkout-promo-code')).toHaveValue(code);
      expect(psql(`SELECT COUNT(*) FROM booking WHERE guest_id = '${guest.id}';`)).toBe('0');
    });

    test('malformed input is echoed escaped', NEW_ONLY, async ({ page }) => {
      await openCheckout(page);
      const dialogs: string[] = [];
      page.on('dialog', (dialog) => {
        dialogs.push(dialog.message());
        void dialog.dismiss();
      });

      await page.locator('#checkout-promo-code').fill('"><script>alert(1)</script>');
      await page.locator('#checkout-promo-apply').click();

      await expect(page.locator('#checkout-promo-refusal')).toContainText("isn't valid");
      await expect(page.locator('#checkout-promo-code')).toHaveValue('"><script>alert(1)</script>');
      expect(dialogs).toEqual([]);
    });

    test('it works with JavaScript disabled', NEW_ONLY, async ({ browser, baseURL }) => {
      const context = await browser.newContext({ baseURL, javaScriptEnabled: false });
      const page = await context.newPage();
      const guest = await openCheckout(page);
      const code = createPromoCode(guest.id);

      await page.locator('#checkout-promo-code').fill(code);
      await page.locator('#checkout-promo-apply').click();

      await expect(page.locator('#breakdown-promo')).toContainText('325.00');
      await context.close();
    });
  });
});

