import { test, expect } from '../fixtures/auth.fixture.js';
import * as crypto from 'node:crypto';
import { createHost, loginAdminAs } from '../fixtures/admin-data.js';

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

test.describe('Admin Portal - Payout status for hosts', () => {
  // Decision D10 (parity inventory A27): only admins may change a payout
  // status. The legacy page renders the status dialog for hosts too.
  test('shows hosts the ledger without status controls', { tag: '@new-only' }, async ({ page }) => {
    await loginAdminAs(page, createHost());
    const resp = await page.goto('/admin/payouts');
    expect(resp?.status()).toBe(200);
    await expect(page.locator('#payout-status-form')).toHaveCount(0);
    await expect(page.locator('#payout-status-modal')).toHaveCount(0);
  });

  test('rejects a payout status change posted by a host', { tag: '@new-only' }, async ({ page }) => {
    await loginAdminAs(page, createHost());
    await page.goto('/admin/payouts');
    const resp = await page.request.post(`/admin/payouts/${crypto.randomUUID()}/status`, {
      form: { status: 'paid' },
      headers: { Origin: new URL(page.url()).origin },
    });
    expect(resp.status()).toBe(403);
  });
});

