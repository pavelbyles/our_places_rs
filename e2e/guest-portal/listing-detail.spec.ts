import { test, expect } from '@playwright/test';
import { KNOWN_LISTINGS } from '../fixtures/test-data.js';

test.describe('Guest Portal - Listing Detail & Dynamic Pricing', () => {
  const targetVilla = KNOWN_LISTINGS.courtyardStudio;

  test('should display live listing details for The Courtyard Studio', async ({ page }) => {
    const response = await page.goto(`/listings/${targetVilla.slug}`);
    expect(response?.status()).toBe(200);

    // Fail if listing not found notice is displayed
    await expect(page.locator('text=Listing Not Found')).not.toBeVisible();

    // 1. Heading and villa title
    const heading = page.locator('h1');
    await expect(heading).toBeVisible();
    await expect(heading).toHaveText(targetVilla.name);

    // 2. Location and pricing badge
    await expect(page.locator('text=New Kingston, Jamaica').first()).toBeVisible();
    await expect(page.locator('text=USD 650').first()).toBeVisible();

    // 3. Amenities section
    await expect(page.locator('text=What this place offers')).toBeVisible();

    // 4. Reserve button with 15-min hold indicator linking to checkout
    const reserveBtn = page.locator(`a[href="/checkout/${targetVilla.slug}"]`).first();
    await expect(reserveBtn).toBeVisible();
    await expect(reserveBtn).toContainText('Reserve (15-Min Hold)');
  });

  test('should recalculate dynamic price and statutory GCT on date selection', async ({ page }) => {
    const response = await page.goto(`/listings/${targetVilla.slug}`);
    expect(response?.status()).toBe(200);

    const checkInInput = page.locator('#reserve-check-in');
    const checkOutInput = page.locator('#reserve-check-out');

    await expect(checkInInput).toBeVisible();
    await expect(checkOutInput).toBeVisible();

    // Set 5-night stay dates starting from a known future date
    const now = new Date();
    const inDate = new Date(now.getTime() + 2 * 86400000);
    const outDate = new Date(inDate.getTime() + 5 * 86400000);

    const inStr = inDate.toISOString().split('T')[0];
    const outStr = outDate.toISOString().split('T')[0];

    await checkInInput.fill(inStr);
    await checkOutInput.fill(outStr);
    await checkOutInput.dispatchEvent('change');

    // Verify quote breakdown updates
    const breakdown = page.locator('#quote-breakdown');
    await expect(breakdown).toBeVisible();

    // Verify statutory GCT (15%) is present in breakdown
    await expect(breakdown).toContainText(/15%|GCT|Tax/i);
    await expect(breakdown).toContainText(/5 night/i);
  });
});
