import { test, expect } from '../fixtures/base.fixture.js';

test.describe('Guest Portal - Home Page', () => {
  test('should render the hero section and luxury branding', async ({ page }) => {
    await page.goto('/');
    await expect(page).toHaveTitle(/Our Places/i);

    const heroHeading = page.locator('h1');
    await expect(heroHeading).toBeVisible();
    await expect(heroHeading).toContainText("Jamaica's Finest Escapes");
    await expect(page.locator('a[href="/listings"]').first()).toBeVisible();
  });
});
