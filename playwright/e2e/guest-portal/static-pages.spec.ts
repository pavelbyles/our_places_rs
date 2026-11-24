import { test, expect } from '../fixtures/base.fixture.js';

test.describe('Guest Portal - Static Pages', () => {
  test('renders the About page', async ({ page }) => {
    const resp = await page.goto('/about');
    expect(resp?.status()).toBe(200);
    await expect(page.locator('h1')).toBeVisible();
    await expect(page.getByRole('main')).toContainText('Our Places');
  });

  for (const [path, heading] of [
    ['/terms', 'Terms and conditions'],
    ['/cancellation-policy', 'Cancellation policy'],
    ['/privacy-policy', 'Privacy policy'],
    ['/house-rules', 'House rules'],
  ]) {
    test(`renders the ${heading} page, marked as a draft`, async ({ page }) => {
      const resp = await page.goto(path);
      expect(resp?.status()).toBe(200);
      await expect(page.locator('h1')).toHaveText(heading);
      await expect(page.getByTestId('policy-draft-notice')).toContainText('placeholder text');
    });
  }

  test('the footer links every policy page', async ({ page }) => {
    await page.goto('/about');
    const footer = page.getByTestId('site-footer');
    for (const path of ['/terms', '/cancellation-policy', '/privacy-policy', '/house-rules']) {
      await expect(footer.locator(`a[href="${path}"]`)).toBeVisible();
    }
  });
});
