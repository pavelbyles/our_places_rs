import { test, expect } from '../fixtures/auth.fixture.js';

test.describe('Admin Portal - Exchange Rates', () => {
  test('shows the tri-currency rate table', async ({ authenticatedAdminPage: page }) => {
    const resp = await page.goto('/admin/exchange-rates');
    expect(resp?.status()).toBe(200);
    await expect(page.locator('h1')).toContainText('Exchange Rates');
    await expect(page.getByRole('main')).toContainText('USD / JMD');
    await expect(page.getByRole('main')).toContainText('USD / EUR');
  });

  // The legacy table is hardcoded; the redesign reads currency_exchange_rates.
  test('shows the stored USD / JMD rate', { tag: '@new-only' }, async ({ authenticatedAdminPage: page }) => {
    await page.goto('/admin/exchange-rates');
    await expect(page.getByRole('alert')).toHaveCount(0);
    const row = page.locator('#exchange-rates-table tr', { hasText: 'USD / JMD' });
    await expect(row).toContainText(/\d+\.\d{6}/);
  });
});
