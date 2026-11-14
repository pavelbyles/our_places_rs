import { test, expect } from '../fixtures/base.fixture.js';
import { KNOWN_LISTINGS, KNOWN_LOCATION } from '../fixtures/test-data.js';
import { SWIFTSHADER, watchMapRequests } from '../fixtures/map-watch.js';

/**
 * Spec 103 §1.6 on the running guest portal. Needs the portal started with
 * MAP_TILES_BASE_URL (run `map-tiles-seed`, then restart the frontends) and
 * the seeded coordinates from db-seed.
 */

test.use(SWIFTSHADER);

const target = KNOWN_LISTINGS.courtyardStudio;

test.describe('Guest Portal - listings map', () => {
  test('desktop: one badge per mapped card, popups, keyboard and attribution', async ({ page }) => {
    const watch = watchMapRequests(page);
    const search = 'check_in=2030-08-01&check_out=2030-08-04&guests=2';
    await page.goto(`/listings?${search}`);
    const map = page.locator('[data-map="search"]');
    await expect(map).toBeVisible();
    await expect(map).toHaveAttribute('data-map-state', 'ready', { timeout: 30000 });
    const mapped = await page.locator('#listings-grid [data-map-lat]').count();
    expect(mapped).toBeGreaterThan(0);
    await expect(map.locator('[data-map-badge] button')).toHaveCount(mapped);
    await expect(map.locator('.maplibregl-ctrl-attrib')).toContainText('OpenStreetMap contributors');

    const badge = map.getByRole('button', { name: new RegExp(`^${target.name}, `) });
    await badge.click();
    const link = page.locator('.maplibregl-popup a').first();
    await expect(link).toHaveAttribute('href', `/listings/${target.slug}?${search}`);

    // Keyboard: Escape closes and returns focus; Enter opens again.
    await page.keyboard.press('Escape');
    await expect(page.locator('.maplibregl-popup')).toHaveCount(0);
    await expect(badge).toBeFocused();
    await page.keyboard.press('Enter');
    await expect(page.locator('.maplibregl-popup')).toHaveCount(1);
    await page.locator('.maplibregl-popup a').first().click();
    await expect(page).toHaveURL(new RegExp(`/listings/${target.slug}\\?`));
    expect(watch.failed).toEqual([]);
  });

  test('phone: nothing map-related loads before the toggle, and ?view=map survives a reload', async ({ page }) => {
    await page.setViewportSize({ width: 390, height: 844 });
    const watch = watchMapRequests(page);
    await page.goto('/listings');
    await expect(page.locator('#listings-grid')).toBeVisible();
    const map = page.locator('[data-map="search"]');
    await expect(map).toBeHidden();
    await page.waitForTimeout(500);
    expect(watch.requests).toEqual([]);

    await page.getByRole('link', { name: 'Map', exact: true }).click();
    await expect(page).toHaveURL(/view=map/);
    await expect(page.locator('#listings-grid')).toBeHidden();
    await expect(map).toHaveAttribute('data-map-state', 'ready', { timeout: 30000 });

    await page.reload();
    await expect(page.locator('#listings-grid')).toBeHidden();
    await expect(page.locator('[data-map="search"]')).toBeVisible();
    await expect(page.getByRole('link', { name: 'Map', exact: true })).toHaveAttribute('aria-current', 'page');
    await expect(page.locator('[data-map="search"]')).toHaveAttribute('data-map-state', 'ready', { timeout: 30000 });
    expect(watch.failed).toEqual([]);
  });

  test("/listings never carries an approximate listing's stored point", async ({ page }) => {
    const response = await page.goto('/listings');
    const html = (await response?.text()) ?? '';
    expect(html).toContain(`listing-card-${target.slug}`);
    for (const stored of [KNOWN_LOCATION.latitude, KNOWN_LOCATION.longitude]) {
      expect(html).not.toContain(String(stored));
    }
    await expect(page.locator(`#listing-card-${target.slug}`)).toHaveAttribute('data-map-precision', 'approximate');
  });
});
