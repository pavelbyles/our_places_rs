import { test, expect } from '@playwright/test';
import { KNOWN_LISTINGS } from '../fixtures/test-data.js';

test.describe('Guest Portal - Listings Exploration Page', () => {
  const targetVilla = KNOWN_LISTINGS.courtyardStudio;

  test('should display active listings including The Courtyard Studio', async ({ page }) => {
    const response = await page.goto('/listings');
    expect(response?.status()).toBe(200);

    // Main headings and filters
    await expect(page.locator('h1')).toHaveText('Explore All Villas');
    await expect(page.locator('input[placeholder*="Search villa name"]')).toBeVisible();

    // Listings grid should contain the seeded villa
    const grid = page.locator('#listings-grid');
    await expect(grid).toBeVisible();

    // Verify villa card contents
    const villaCard = grid.locator('.card', { hasText: targetVilla.name });
    await expect(villaCard).toBeVisible();
    await expect(villaCard).toContainText('USD 650');
    await expect(villaCard).toContainText('New Kingston, Jamaica');
    await expect(villaCard).toContainText('4 Guests');

    // Verify link to detail page
    const detailLink = villaCard.locator(`a[href="/listings/${targetVilla.slug}"]`).first();
    await expect(detailLink).toBeVisible();
  });

  test('should navigate to listing detail page when clicking View Villa', async ({ page }) => {
    await page.goto('/listings');

    const villaCard = page.locator('#listings-grid .card', { hasText: targetVilla.name });
    const viewBtn = villaCard.getByRole('link', { name: 'View Villa' });
    await expect(viewBtn).toBeVisible();

    await viewBtn.click();
    await expect(page).toHaveURL(`/listings/${targetVilla.slug}`);
    await expect(page.locator('h1')).toHaveText(targetVilla.name);
  });
});
