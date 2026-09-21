import { test, expect } from '@playwright/test';
import { KNOWN_LISTINGS, TEST_USERS } from '../fixtures/test-data.js';

test.describe('Guest Portal - Checkout Flow & 15-Minute Hold', () => {
  const targetVilla = KNOWN_LISTINGS.courtyardStudio;

  test('should display checkout page with 15-minute reservation timer and price breakdown', async ({ page }) => {
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
    await expect(page.locator('.card', { hasText: targetVilla.name })).toBeVisible();
    await expect(page.locator('text=New Kingston, Jamaica').first()).toBeVisible();

    // 3. Verify price breakdown and 15% statutory GCT
    await expect(page.locator('#breakdown-subtotal')).toContainText('3250.00');
    await expect(page.locator('#breakdown-tax')).toContainText('487.50');
    await expect(page.locator('#breakdown-total')).toContainText('3737.50');

    // 4. Verify guest form inputs are present
    await expect(page.locator('#guest-first-name')).toBeVisible();
    await expect(page.locator('#guest-last-name')).toBeVisible();
    await expect(page.locator('#guest-email')).toBeVisible();
    await expect(page.locator('#guest-phone')).toBeVisible();
  });

  test('should require mandatory fields before proceeding to payment', async ({ page }) => {
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

  test('should fill valid guest details smoothly', async ({ page }) => {
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
});
