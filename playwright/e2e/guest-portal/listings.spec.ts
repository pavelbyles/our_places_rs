import { test, expect } from '../fixtures/base.fixture.js';
import { KNOWN_LISTINGS } from '../fixtures/test-data.js';
import { psql } from '../fixtures/bookings-data.js';

test.describe('Guest Portal - Listings Exploration Page', () => {
  const targetVilla = KNOWN_LISTINGS.courtyardStudio;

  test('should display active listings including The Courtyard Studio', async ({ page }) => {
    const response = await page.goto('/listings');
    expect(response?.status()).toBe(200);

    // Main headings and filters
    await expect(page.locator('h1')).toHaveText('Explore All Listings');
    await expect(page.locator('input[placeholder*="Search listing name"]')).toBeVisible();

    // Listings grid should contain the seeded villa
    const grid = page.locator('#listings-grid');
    await expect(grid).toBeVisible();

    // Verify villa card contents
    const villaCard = grid.locator(`#listing-card-${targetVilla.slug}`);
    await expect(villaCard).toBeVisible();
    await expect(villaCard).toContainText('USD 650');
    await expect(villaCard).toContainText('New Kingston, Jamaica');
    await expect(villaCard).toContainText('4 Guests');

    // Verify link to detail page
    const detailLink = villaCard.locator(`a[href="/listings/${targetVilla.slug}"]`).first();
    await expect(detailLink).toBeVisible();
  });

  test('should navigate to listing detail page when clicking View Listing', async ({ page }) => {
    await page.goto('/listings');

    const villaCard = page.locator(`#listings-grid #listing-card-${targetVilla.slug}`);
    const viewBtn = villaCard.getByRole('link', { name: 'View Listing' });
    await expect(viewBtn).toBeVisible();

    await viewBtn.click();
    await expect(page).toHaveURL(`/listings/${targetVilla.slug}`);
    await expect(page.locator('h1')).toHaveText(targetVilla.name);
  });

  test('links each card to its host\'s listings', async ({ page }) => {
    const [hostSlug, hostName] = psql(`
      SELECT u.slug || '|' || u.first_name || ' ' || u.last_name
      FROM listing l JOIN "user" u ON u.id = l.user_id
      WHERE l.slug = '${targetVilla.slug}';`).split('|');

    await page.goto('/listings');
    const hostLink = page.locator(`#listing-card-${targetVilla.slug} [data-host-link]`);
    await expect(hostLink).toHaveText(hostName);
    await expect(hostLink).toHaveAttribute('href', `/listings?host=${hostSlug}`);

    await hostLink.click();
    await expect(page).toHaveURL(new RegExp(`/listings\\?host=${hostSlug}$`));
    await expect(page.locator('h1')).toHaveText(`Listings by ${hostName}`);
    await expect(page.locator(`#listing-card-${targetVilla.slug}`)).toBeVisible();
    const hosts = await page.locator('#listings-grid [data-host-link]').evaluateAll(
      (links) => [...new Set(links.map((a) => a.getAttribute('href')))],
    );
    expect(hosts).toEqual([`/listings?host=${hostSlug}`]);

    await page.locator('#clear-host-filter').click();
    await expect(page).toHaveURL(/\/listings$/);
    await expect(page.locator('h1')).toHaveText('Explore All Listings');
  });

  test('says so when a host slug has no listings', async ({ page }) => {
    await page.goto('/listings?host=no-such-host-e2e');
    await expect(page.locator('h1')).toHaveText('Host listings');
    await expect(page.getByText('This host has no listings yet')).toBeVisible();
  });
});
