import { test, expect } from '../fixtures/auth.fixture.js';

test.describe('Admin Portal - Master Booking Schedule', () => {
  test.beforeEach(async ({ authenticatedAdminPage }) => {
    await authenticatedAdminPage.goto('/admin/bookings');
  });

  test('should render master booking schedule title and export button', async ({ authenticatedAdminPage }) => {
    const heading = authenticatedAdminPage.locator('h1');
    await expect(heading).toContainText('Master Booking Schedule');

    // Export CSV button
    const exportBtn = authenticatedAdminPage.locator('button:has-text("Export CSV")');
    await expect(exportBtn).toBeVisible();
  });
});
