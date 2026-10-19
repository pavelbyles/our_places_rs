import { test, expect } from '../fixtures/auth.fixture.js';

test.describe('Admin Portal - Listing Fees & Statutory Documents Audit', () => {
  test('should display statutory tax documents in booking audit modal', async ({ authenticatedAdminPage }) => {
    await authenticatedAdminPage.goto('/admin/bookings');

    // Click Details button on the first booking
    const detailsBtn = authenticatedAdminPage.locator('button:has-text("Details")').first();
    await expect(detailsBtn).toBeVisible();
    await detailsBtn.click();

    // Verify booking details dialog opens
    const dialog = authenticatedAdminPage.locator('#admin-booking-details-dialog');
    await expect(dialog).toBeVisible();

    // Verify statutory tax documents section is present
    const docsSection = dialog.locator('#admin-booking-documents-section');
    await expect(docsSection).toBeVisible();

    // Wait for documents list to populate (fetches from /api/bookings/{id}/documents)
    const docsList = dialog.locator('#admin-booking-documents-list');
    await expect(docsList).toBeVisible();

    // Verify document badge or items loaded
    const docsBadge = dialog.locator('#detail-docs-count-badge');
    await expect(docsBadge).toBeVisible();
  });

  test('should render statutory financial audit breakdown in booking details modal', async ({ authenticatedAdminPage }) => {
    await authenticatedAdminPage.goto('/admin/bookings');

    const detailsBtn = authenticatedAdminPage.locator('button:has-text("Details")').first();
    await expect(detailsBtn).toBeVisible();
    await detailsBtn.click();

    const dialog = authenticatedAdminPage.locator('#admin-booking-details-dialog');
    await expect(dialog).toBeVisible();

    // Financial audit breakdown elements
    await expect(dialog.locator('text=Statutory Tri-Currency Financial Audit')).toBeVisible();
    await expect(dialog.locator('#detail-subtotal')).toBeVisible();
    await expect(dialog.locator('#detail-tax')).toBeVisible();
    await expect(dialog.locator('#detail-total')).toBeVisible();
  });
});
