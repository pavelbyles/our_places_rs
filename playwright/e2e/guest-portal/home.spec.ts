import { test, expect } from '@playwright/test';

test.describe('Guest Portal - Home Page', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/');
  });

  test('should render the hero section and luxury branding', async ({ page }) => {
    // Check page title
    await expect(page).toHaveTitle(/Our Places/i);

    // Verify main hero heading
    const heroHeading = page.locator('h1');
    await expect(heroHeading).toBeVisible();
    await expect(heroHeading).toContainText("Jamaica's Finest Escapes");

    // Verify luxury villa tagline
    const subtext = page.locator('text=Curated private villas with dedicated butler service');
    await expect(subtext).toBeVisible();
  });

  test('should render navigation bar and essential links', async ({ page }) => {
    const nav = page.locator('nav, header');
    await expect(nav.first()).toBeVisible();

    // Check link to listings
    const exploreLink = page.locator('a[href="/listings"]').first();
    await expect(exploreLink).toBeVisible();
  });

  test('should display luxury experience perks and value propositions', async ({ page }) => {
    // Verify perks section
    await expect(page.locator('text=Dedicated Butler & Staff')).toBeVisible();
  });

  test('should have responsive layout across viewport changes', async ({ page }) => {
    // Mobile viewport
    await page.setViewportSize({ width: 375, height: 667 });
    await expect(page.locator('h1')).toBeVisible();

    // Desktop viewport
    await page.setViewportSize({ width: 1280, height: 800 });
    await expect(page.locator('h1')).toBeVisible();
  });
});
