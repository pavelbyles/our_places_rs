import { test, expect } from '../fixtures/auth.fixture.js';

test.describe('Admin Portal - Host Earnings & Payout Ledger', () => {
  test.beforeEach(async ({ authenticatedAdminPage }) => {
    await authenticatedAdminPage.goto('/admin/payouts');
  });

  test('should render host earnings & payout ledger header and CSV export', async ({ authenticatedAdminPage }) => {
    const heading = authenticatedAdminPage.locator('h1');
    await expect(heading).toContainText('Host Earnings & Payout Ledger');

    // Verify Financial Engine & Tri-Currency tags
    await expect(authenticatedAdminPage.locator('text=Financial Engine')).toBeVisible();
    await expect(authenticatedAdminPage.locator('text=Tri-Currency Ledger')).toBeVisible();

    // Verify CSV export download link
    const exportBtn = authenticatedAdminPage.locator('a[download="host_payout_ledger.csv"]');
    await expect(exportBtn).toBeVisible();
  });

  test('should display ledger summary metrics cards', async ({ authenticatedAdminPage }) => {
    await expect(authenticatedAdminPage.locator('text=Gross Volume')).toBeVisible();
    await expect(authenticatedAdminPage.locator('text=Platform Fee')).toBeVisible();
    await expect(authenticatedAdminPage.locator('text=Net Host Payouts')).toBeVisible();
  });
});
