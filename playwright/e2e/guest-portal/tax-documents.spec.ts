import { test, expect } from '@playwright/test';
import { TEST_USERS } from '../fixtures/test-data.js';

async function loginAsGuest(page: import('@playwright/test').Page) {
  await page.goto('/login');
  await page.locator('#guest-email').fill(TEST_USERS.guest.email);
  await page.locator('#guest-password').fill(TEST_USERS.guest.password);
  await page.locator('#btn-guest-login').click();
  await page.waitForURL((url) => !url.pathname.includes('/login'), { timeout: 15000 });
}

test.describe('Guest Portal - Statutory Tax Documents & Date Modification', () => {
  const bookingId = '018f3a5e-6b9c-7000-8000-000000000010';
  const receiptNumber = 'REC-2026-OP-E2E-7788';

  test('should render statutory payment receipt with itemized GCT, tri-currency FX, and print button', async ({ page }) => {
    await loginAsGuest(page);

    const resp = await page.goto(`/bookings/${bookingId}/documents/${receiptNumber}`);
    expect(resp?.status()).toBe(200);

    // Verify document header and title
    const docContainer = page.locator('.tax-document-container');
    await expect(docContainer).toBeVisible();
    await expect(page.locator('h1, h2, h3', { hasText: 'Payment Receipt' })).toBeVisible();
    await expect(page.locator('text=' + receiptNumber)).toBeVisible();
    await expect(page.locator('.badge', { hasText: 'Issued' })).toBeVisible();

    // Verify statutory Jamaican GCT (15.0%) and line items
    await expect(page.locator('text=15.0%').or(page.locator('text=Jamaican GCT')).first()).toBeVisible();
    await expect(page.locator('text=Accommodation').first()).toBeVisible();
    await expect(page.locator('text=Cleaning fee').or(page.locator('text=Cleaning Fee')).first()).toBeVisible();
    await expect(page.locator('text=Security Damage Deposit').or(page.locator('text=Security deposit')).first()).toBeVisible();

    // Verify JMD equivalent amounts and conversion rate
    await expect(page.locator('text=JMD').first()).toBeVisible();
    await expect(page.locator('text=160.0000 JMD').or(page.locator('text=160.00')).first()).toBeVisible();

    // Verify print button
    const printBtn = page.locator('button:has-text("Print / Download PDF")');
    await expect(printBtn).toBeVisible();
  });

  test('should return 404 for unauthenticated or unauthorized users accessing documents', async ({ browser }) => {
    const unauthedContext = await browser.newContext();
    const page = await unauthedContext.newPage();

    const resp = await page.goto(`/bookings/${bookingId}/documents/${receiptNumber}`);
    // IDOR protection: returns 404 Not Found to avoid leaking existence
    expect(resp?.status()).toBe(404);
    await unauthedContext.close();
  });

  test('should render interactive date modification view for confirmed booking', async ({ page }) => {
    await loginAsGuest(page);

    const resp = await page.goto(`/bookings/${bookingId}/change-dates`);
    expect(resp?.status()).toBe(200);

    await expect(page.locator('h1', { hasText: 'Modify Reservation Dates' })).toBeVisible();
    await expect(page.locator('text=Reservation #OP-E2E-7788')).toBeVisible();

    // Check-in and check-out date inputs
    const checkInInput = page.locator('#new-check-in');
    const checkOutInput = page.locator('#new-check-out');
    await expect(checkInInput).toBeVisible();
    await expect(checkOutInput).toBeVisible();

    // Submit button
    const submitBtn = page.locator('#btn-submit-date-change');
    await expect(submitBtn).toBeVisible();
  });
});
