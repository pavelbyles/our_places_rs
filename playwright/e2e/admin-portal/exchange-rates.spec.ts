import { test, expect } from '../fixtures/auth.fixture.js';

test.describe('Admin Portal - Exchange Rates', () => {
  test('shows the tri-currency rate table', async ({ authenticatedAdminPage: page }) => {
    const resp = await page.goto('/admin/exchange-rates');
    expect(resp?.status()).toBe(200);
    await expect(page.locator('h1')).toContainText('Exchange Rates');
    await expect(page.getByRole('main')).toContainText('USD / JMD');
    await expect(page.getByRole('main')).toContainText('USD / EUR');
  });
});
