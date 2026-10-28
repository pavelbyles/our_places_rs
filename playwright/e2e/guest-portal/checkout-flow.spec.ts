import { test, expect } from '../fixtures/base.fixture.js';
import { KNOWN_LISTINGS, TEST_USERS } from '../fixtures/test-data.js';
import { createGuest, loginGuest, psql } from '../fixtures/bookings-data.js';

// Legacy deviations (parity inventory G8/G13, decisions D1/D2): the legacy
// checkout is open to anonymous visitors, shows a fixed 5-night price and a
// fake "15 minutes" hold countdown, and posts hardcoded dates from the
// browser. Those tests are @old-only; the redesigned flow is tested below.
const OLD_ONLY = { tag: '@old-only' };
const NEW_ONLY = { tag: '@new-only' };

test.describe('Guest Portal - Checkout Flow & 15-Minute Hold', () => {
  const targetVilla = KNOWN_LISTINGS.courtyardStudio;

  test('should display checkout page with 15-minute reservation timer and price breakdown', OLD_ONLY, async ({ page }) => {
    const response = await page.goto(`/checkout/${targetVilla.slug}`);
    expect(response?.status()).toBe(200);

    // Fail if session expired notice is shown
    await expect(page.locator('text=Checkout Session Expired')).not.toBeVisible();

    // 1. Verify 15-minute reservation hold banner and countdown timer
    await expect(page.locator('text=Temporary Date Hold Active')).toBeVisible();
    await expect(page.locator('text=These dates are held exclusively for you in PostgreSQL for 15 minutes.')).toBeVisible();

    const timerBadge = page.locator('#hold-timer-display');
    await expect(timerBadge).toBeVisible();
    await expect(timerBadge).toHaveText(/\d{1,2}:\d{2}\s+Remaining/);

    // 2. Verify listing summary card
    await expect(page.getByRole('main').getByText(targetVilla.name).first()).toBeVisible();
    await expect(page.locator('text=New Kingston, Jamaica').first()).toBeVisible();

    // 3. Verify price breakdown, cleaning fee, damage deposit, and 15% statutory GCT
    await expect(page.locator('#breakdown-subtotal')).toContainText('3250.00');
    await expect(page.locator('#breakdown-cleaning-fee')).toContainText('75.00');
    await expect(page.locator('#breakdown-tax')).toContainText('498.75');
    await expect(page.locator('#breakdown-damage-deposit')).toContainText('150.00');
    await expect(page.locator('#breakdown-total')).toContainText('3973.75');

    // 4. Verify guest form inputs are present
    await expect(page.locator('#guest-first-name')).toBeVisible();
    await expect(page.locator('#guest-last-name')).toBeVisible();
    await expect(page.locator('#guest-email')).toBeVisible();
    await expect(page.locator('#guest-phone')).toBeVisible();
  });

  test('should require mandatory fields before proceeding to payment', OLD_ONLY, async ({ page }) => {
    await page.goto(`/checkout/${targetVilla.slug}`);

    const emailInput = page.locator('#guest-email');
    await expect(emailInput).toBeVisible();

    // Clear email and test HTML5 validation
    await emailInput.fill('');
    const submitBtn = page.locator('#checkout-submit-btn');
    await expect(submitBtn).toBeVisible();

    await submitBtn.click();
    const isInvalid = await emailInput.evaluate((el: HTMLInputElement) => !el.checkValidity());
    expect(isInvalid).toBe(true);
  });

  test('should fill valid guest details smoothly', OLD_ONLY, async ({ page }) => {
    await page.goto(`/checkout/${targetVilla.slug}`);

    const emailInput = page.locator('#guest-email');
    await expect(emailInput).toBeVisible();

    await page.locator('#guest-first-name').fill(TEST_USERS.guest.firstName);
    await page.locator('#guest-last-name').fill(TEST_USERS.guest.lastName);
    await emailInput.fill(TEST_USERS.guest.email);
    await page.locator('#guest-phone').fill(TEST_USERS.guest.phone);

    await expect(page.locator('#guest-first-name')).toHaveValue(TEST_USERS.guest.firstName);
    await expect(page.locator('#guest-last-name')).toHaveValue(TEST_USERS.guest.lastName);
    await expect(emailInput).toHaveValue(TEST_USERS.guest.email);
    await expect(page.locator('#guest-phone')).toHaveValue(TEST_USERS.guest.phone);
  });

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

