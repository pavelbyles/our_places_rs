import { test, expect } from '../fixtures/base.fixture.js';

test.describe('Guest Portal - Static Pages', () => {
  test('renders the About page', async ({ page }) => {
    const resp = await page.goto('/about');
    expect(resp?.status()).toBe(200);
    await expect(page.locator('h1')).toBeVisible();
    await expect(page.getByRole('main')).toContainText('Our Places');
  });
});
