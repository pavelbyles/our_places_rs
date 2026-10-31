import { test, expect } from '../fixtures/auth.fixture.js';
import { createHost, loginAdminAs } from '../fixtures/admin-data.js';

// Parity inventory A20: the legacy admin's booking-details dialog never opens
// (its inline script throws "SyntaxError: Unexpected token ';'", so
// openAdminBookingDetailsFromBtn is undefined). These run on the new portal only.
const NEW_ONLY = { tag: '@new-only' };

test.describe('Admin Portal - Master Booking Schedule & Keyless Entry', () => {
  test.beforeEach(async ({ authenticatedAdminPage }) => {
    await authenticatedAdminPage.goto('/admin/bookings');
  });

  test('should render master booking schedule title and export button', async ({ authenticatedAdminPage }) => {
    const heading = authenticatedAdminPage.locator('h1');
    await expect(heading).toContainText('Master Booking Schedule');

    // Export CSV button
    const exportBtn = authenticatedAdminPage.locator('button:has-text("Export CSV")');
    await expect(exportBtn).toBeVisible();

    // Verify bookings table is rendered
    const table = authenticatedAdminPage.locator('#admin-bookings-table');
    await expect(table).toBeVisible();
  });

  test('should open reservation audit modal and display Keyless Access configuration panel', NEW_ONLY, async ({ authenticatedAdminPage }) => {
    // Find the details button on the first booking row
    const detailsBtn = authenticatedAdminPage.locator('button:has-text("Details")').first();
    await expect(detailsBtn).toBeVisible();

    // Open audit modal
    await detailsBtn.click();

    // Dialog should be open
    const dialog = authenticatedAdminPage.locator('#admin-booking-details-dialog');
    await expect(dialog).toBeVisible();

    // Keyless Access section should be present
    const keylessHeader = dialog.locator('h4:has-text("Door Access Code")');
    await expect(keylessHeader).toBeVisible();

    const doorCodeInput = dialog.locator('#detail-door-code-input');
    await expect(doorCodeInput).toBeVisible();

    const saveBtn = dialog.locator('#btn-save-door-code');
    await expect(saveBtn).toBeVisible();

    const badge = dialog.locator('#detail-door-code-badge');
    await expect(badge).toBeVisible();
  });

  test('should allow host/admin to set, save, and persist a door access code', NEW_ONLY, async ({ authenticatedAdminPage }) => {
    const detailsBtn = authenticatedAdminPage.locator('button:has-text("Details")').first();
    await expect(detailsBtn).toBeVisible();

    await detailsBtn.click();

    const dialog = authenticatedAdminPage.locator('#admin-booking-details-dialog');
    await expect(dialog).toBeVisible();

    const doorCodeInput = dialog.locator('#detail-door-code-input');
    const saveBtn = dialog.locator('#btn-save-door-code');
    const badge = dialog.locator('#detail-door-code-badge');
    const statusMsg = dialog.locator('#door-code-status-msg');

    // Enter a new door access code
    const testCode = `KEY-${Math.floor(1000 + Math.random() * 9000)}`;
    await doorCodeInput.fill(testCode);

    // Click Save Code
    await saveBtn.click();

    // Status message and badge should reflect successful configuration
    await expect(badge).toContainText(`Configured (${testCode})`);
    await expect(badge).toHaveAttribute('data-state', 'configured');
    await expect(statusMsg).toBeVisible();
    await expect(statusMsg).toContainText('Door access code updated successfully');

    // Close the audit modal
    const closeBtn = dialog.locator('button:has-text("Close Audit View")');
    await closeBtn.click();
    await expect(dialog).not.toHaveAttribute('open');

    // The details button on the table should now have the updated data-door-code attribute
    await expect(detailsBtn).toHaveAttribute('data-door-code', testCode);

    // Reopen modal and verify code remains populated
    await detailsBtn.click();
    await expect(dialog).toHaveAttribute('open');
    await expect(doorCodeInput).toHaveValue(testCode);
    await expect(badge).toContainText(`Configured (${testCode})`);
  });

  test('shows admins the refund quote in the details dialog', NEW_ONLY, async ({ authenticatedAdminPage: page }) => {
    await page.locator('button:has-text("Details")').first().click();
    const dialog = page.locator('#admin-booking-details-dialog');
    await dialog.getByRole('button', { name: 'Show refund quote' }).click();
    await expect(dialog.locator('#admin-cancellation-quote')).toContainText('Refund total');
  });

  test('exports the schedule as CSV', NEW_ONLY, async ({ authenticatedAdminPage: page }) => {
    const res = await page.request.get('/admin/bookings/export.csv');
    expect(res.status()).toBe(200);
    expect(res.headers()['content-type']).toContain('text/csv');
    expect((await res.text()).split('\r\n')[0]).toBe('confirmation_code,listing,check_in,check_out,guests,status,currency,total');
  });

  test("keeps hosts out of other listings' bookings and refund quotes", NEW_ONLY, async ({ browser }) => {
    const context = await browser.newContext({ baseURL: 'http://localhost:3012' });
    const page = await context.newPage();
    await loginAdminAs(page, createHost());
    const seeded = '018f3a5e-6b9c-7000-8000-000000000010';
    expect((await page.goto(`/admin/bookings/${seeded}`))?.status()).toBe(404);
    expect((await page.goto(`/admin/bookings/${seeded}/cancellation-quote`))?.status()).toBe(403);
    await page.goto('/admin/bookings');
    await expect(page.locator('#admin-bookings-table')).not.toContainText('OP-E2E-7788');
    await context.close();
  });
});

