import * as crypto from 'node:crypto';
import { test, expect } from '../fixtures/auth.fixture.js';
import { createHost, loginAdminAs } from '../fixtures/admin-data.js';
import { psql } from '../fixtures/bookings-data.js';

const NEW_ONLY = { tag: '@new-only' };
const uniqueCode = () => `E2E-${crypto.randomUUID().slice(0, 8).toUpperCase()}`;
const today = new Date();
const addDays = (n: number) => new Date(today.getTime() + n * 86_400_000).toISOString().slice(0, 10);

test.describe('Admin Portal - Promo Codes', () => {
  test('an admin creates, lists and deactivates a code', NEW_ONLY, async ({ authenticatedAdminPage: page }) => {
    const code = uniqueCode();
    await page.goto('/admin/promo-codes');
    await page.locator('#promo-code').fill(code);
    await page.locator('#promo-discount-value').fill('15');
    await page.locator('#promo-valid-from').fill(addDays(0));
    await page.locator('#promo-valid-to').fill(addDays(30));
    await page.locator('#promo-total-limit').fill('5');
    await page.locator('#promo-code-submit').click();

    const row = page.locator(`[data-promo-code="${code}"]`);
    await expect(row).toBeVisible();
    await expect(row).toContainText('15%');
    await expect(row.locator('[data-promo-status]')).toHaveText('Active');
    await expect(row).toContainText('5');
    expect(psql(`SELECT owner_host_id IS NULL FROM promo_code WHERE code = '${code}';`)).toBe('t');

    await row.getByRole('button', { name: 'Deactivate' }).click();
    await expect(row.locator('[data-promo-status]')).toHaveText('Deactivated');
    await expect(row.getByRole('button', { name: 'Deactivate' })).toHaveCount(0);
    expect(psql(`SELECT deactivated_at IS NOT NULL FROM promo_code WHERE code = '${code}';`)).toBe('t');
  });

  test('refused terms show the reason and keep what was typed', NEW_ONLY, async ({ authenticatedAdminPage: page }) => {
    const code = uniqueCode();
    await page.goto('/admin/promo-codes');
    await page.locator('#promo-code').fill(code);
    await page.locator('#promo-discount-value').fill('33.335');
    await page.locator('#promo-valid-from').fill(addDays(0));
    await page.locator('#promo-valid-to').fill(addDays(30));
    await page.locator('#promo-code-submit').click();

    await expect(page.locator('#promo-code-error')).toContainText('2 decimal places');
    await expect(page.locator('#promo-code')).toHaveValue(code);
    await expect(page.locator('#promo-discount-value')).toHaveValue('33.335');
  });

  test('a duplicate in another case shows the API refusal', NEW_ONLY, async ({ authenticatedAdminPage: page }) => {
    const code = uniqueCode();
    for (const typed of [code, code.toLowerCase()]) {
      await page.goto('/admin/promo-codes');
      await page.locator('#promo-code').fill(typed);
      await page.locator('#promo-discount-value').fill('10');
      await page.locator('#promo-valid-from').fill(addDays(0));
      await page.locator('#promo-valid-to').fill(addDays(30));
      await page.locator('#promo-code-submit').click();
    }
    await expect(page.locator('#promo-code-error')).toContainText('already exists');
  });

  test('a host sees only their own codes and listings', NEW_ONLY, async ({ page, authenticatedAdminPage }) => {
    const otherCode = uniqueCode();
    await authenticatedAdminPage.goto('/admin/promo-codes');
    await authenticatedAdminPage.locator('#promo-code').fill(otherCode);
    await authenticatedAdminPage.locator('#promo-discount-value').fill('10');
    await authenticatedAdminPage.locator('#promo-valid-from').fill(addDays(0));
    await authenticatedAdminPage.locator('#promo-valid-to').fill(addDays(30));
    await authenticatedAdminPage.locator('#promo-code-submit').click();
    await expect(authenticatedAdminPage.locator(`[data-promo-code="${otherCode}"]`)).toBeVisible();

    const host = createHost();
    const listingName = `E2E listing ${crypto.randomUUID()}`;
    const listingId = crypto.randomUUID();
    psql(`INSERT INTO listing (id, name, listing_structure_id, country, added_at, user_id, slug, is_active)
          VALUES ('${listingId}', '${listingName}', 1, 'Jamaica', NOW(), '${host.id}', 'e2e-${listingId}', TRUE);`);
    const context = await page.context().browser()!.newContext({ baseURL: new URL(authenticatedAdminPage.url()).origin });
    const hostPage = await context.newPage();
    await loginAdminAs(hostPage, host);

    await hostPage.goto('/admin/promo-codes');
    await expect(hostPage.locator(`[data-promo-code="${otherCode}"]`)).toHaveCount(0);
    const picker = hostPage.locator('#promo-listing-picker');
    await expect(picker.locator('input[type="checkbox"]')).toHaveCount(1);
    await expect(picker).toContainText(listingName);

    const mine = uniqueCode();
    await hostPage.locator('#promo-code').fill(mine);
    await hostPage.locator('#promo-discount-value').fill('20');
    await hostPage.locator('#promo-valid-from').fill(addDays(0));
    await hostPage.locator('#promo-valid-to').fill(addDays(30));
    await hostPage.locator('#promo-applies-selected').check();
    await picker.locator('input[type="checkbox"]').check();
    await hostPage.locator('#promo-code-submit').click();

    const row = hostPage.locator(`[data-promo-code="${mine}"]`);
    await expect(row).toBeVisible();
    await expect(row).toContainText('1 selected');
    expect(psql(`SELECT owner_host_id FROM promo_code WHERE code = '${mine}';`)).toBe(host.id);
    await context.close();
  });
});
