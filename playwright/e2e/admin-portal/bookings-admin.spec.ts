import { test, expect } from '../fixtures/auth.fixture.js';

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

  test('should open reservation audit modal and display Keyless Access configuration panel', async ({ authenticatedAdminPage }) => {
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

  test('should allow host/admin to set, save, and persist a door access code', async ({ authenticatedAdminPage }) => {
    const detailsBtn = authenticatedAdminPage.locator('button:has-text("Details")').first();
    await expect(detailsBtn).toBeVisible();

    await detailsBtn.click();

    const dialog = authenticatedAdminPage.locator('#admin-booking-details-dialog');
    await expect(dialog).toBeVisible();

    const doorCodeInput = dialog.locator('#detail-door-code-input');
    const saveBtn = dialog.locator('#btn-save-door-code');
    const badge = dialog.locator('#detail-door-code-badge');
    const statusMsg = dialog.locator('#door-code-status-msg');

    // Enter a new door code
    const testCode = '8822-KEY';
    await doorCodeInput.fill(testCode);

    // Click Save Code
    await saveBtn.click();

    // Status message should indicate success
    await expect(statusMsg).toBeVisible();
    await expect(statusMsg).toContainText('Door access code updated successfully');

    // Badge should reflect the configured code
    await expect(badge).toContainText(`Configured (${testCode})`);
    await expect(badge).toHaveClass(/badge-success/);

    // Close the audit modal
    const closeBtn = dialog.locator('button:has-text("Close Audit View")');
    await closeBtn.click();

    // The details button on the table should now have the updated data-door-code attribute
    await expect(detailsBtn).toHaveAttribute('data-door-code', testCode);

    // Reopen modal and verify code remains populated
    await detailsBtn.click();
    await expect(doorCodeInput).toHaveValue(testCode);
    await expect(badge).toContainText(`Configured (${testCode})`);
  });
});
