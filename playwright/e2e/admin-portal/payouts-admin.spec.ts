import { test, expect } from '../fixtures/auth.fixture.js';
import * as crypto from 'node:crypto';
import { createHost, loginAdminAs } from '../fixtures/admin-data.js';
import { psql } from '../fixtures/bookings-data.js';

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

test.describe('Admin Portal - Promo discounts on the ledger', () => {
  test('a host sees the code, discount and cleaning fee, and in the export', { tag: '@new-only' }, async ({ page }) => {
    const host = createHost();
    const guestId = crypto.randomUUID();
    const listingId = crypto.randomUUID();
    const code = `E2E-${crypto.randomUUID().slice(0, 8).toUpperCase()}`;
    const confirmation = crypto.randomUUID().slice(0, 8).toUpperCase();
    psql(`
      INSERT INTO "user" (id, email, password_hash, first_name, last_name, roles, is_active, is_verified, default_currency, created_at, updated_at)
        VALUES ('${guestId}', 'e2e.guest.${guestId.slice(0, 8)}@example.com', 'x', 'Casey', 'E2E', '{booker}', true, true, 'USD', NOW(), NOW());
      INSERT INTO listing (id, name, listing_structure_id, country, added_at, user_id, slug, is_active)
        VALUES ('${listingId}', 'E2E listing ${listingId}', 1, 'Jamaica', NOW(), '${host.id}', 'e2e-${listingId}', TRUE);
      INSERT INTO promo_code (code, discount_type, discount_value, scope, valid_from, valid_to, applies_to_all, created_by)
        VALUES ('${code}', 'percentage', 10, 'stay', NOW(), NOW() + INTERVAL '1 day', TRUE, '${host.id}');
      WITH b AS (
        INSERT INTO booking (confirmation_code, guest_id, listing_id, date_from, date_to, daily_rate, number_of_persons,
            total_days, sub_total_price, total_price, cancellation_policy, status, promo_code_id, promo_terms, promo_stay_discount)
          VALUES ('${confirmation}', '${guestId}', '${listingId}', '2039-01-01', '2039-01-03', 200, 1, 2, 400, 360, 'flexible', 'confirmed',
            (SELECT id FROM promo_code WHERE code = '${code}'),
            '{"code":"${code}","discount_type":"percentage","discount_value":"10","scope":"stay","currency":null,"exchange_rate":"1","min_nights":null,"min_booking_value":null,"stay_from":null,"stay_to":null}', 40)
          RETURNING id)
      INSERT INTO host_payout_ledger (booking_id, listing_id, host_id, gross_amount, net_payout_amount, cleaning_fee_amount, promo_discount_amount)
        SELECT id, '${listingId}', '${host.id}', 410, 369, 50, 40 FROM b;`);

    await loginAdminAs(page, host);
    await page.goto('/admin/payouts');
    const row = page.locator('#payouts-ledger-table tbody tr', { hasText: confirmation });
    await expect(row.locator('[data-payout-promo]')).toContainText(code);
    await expect(row.locator('[data-payout-promo]')).toContainText('40.00');
    await expect(row).toContainText('50.00');

    const exported = await page.request.get('/admin/payouts/export.csv');
    const csv = await exported.text();
    expect(csv.split('\r\n')[0]).toContain('cleaning_fee,promo_code,promo_discount');
    expect(csv).toContain(`"50.00","${code}","40.00"`);
  });
});

