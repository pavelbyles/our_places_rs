import { test, expect } from '../fixtures/base.fixture.js';
import { EXACT_LOCATION, KNOWN_LISTINGS, KNOWN_LOCATION } from '../fixtures/test-data.js';
import { createListingCopy, psql } from '../fixtures/bookings-data.js';

/**
 * Spec 103 §1.6 and §1.7 on the running guest portal. Needs the portal
 * started with MAP_TILES_BASE_URL (run `map-tiles-seed`, then restart the
 * frontends) and the seeded coordinates from db-seed.
 */

const MAP_HOSTS = /maplibre|pmtiles|map-[0-9a-f]+\.js|map-tiles|\.pmtiles|style-(light|dark)\.json/;

test.use({
  launchOptions: {
    args: ['--enable-unsafe-swiftshader', '--use-angle=swiftshader', '--ignore-gpu-blocklist'],
  },
});

test.describe('Guest Portal - listings map', () => {
  test('phones load nothing map-related until the Map view is shown', async ({ page }) => {
    await page.setViewportSize({ width: 390, height: 844 });
    const requests: string[] = [];
    const failed: string[] = [];
    page.on('request', (r) => {
      if (MAP_HOSTS.test(r.url())) requests.push(r.url());
    });
    page.on('requestfailed', (r) => {
      if (MAP_HOSTS.test(r.url())) failed.push(`${r.url()} ${r.failure()?.errorText}`);
    });
    page.on('response', (r) => {
      if (MAP_HOSTS.test(r.url()) && r.status() >= 400) failed.push(`${r.url()} ${r.status()}`);
    });

    await page.goto('/listings');
    await expect(page.locator('#listings-grid')).toBeVisible();
    const map = page.locator('[data-map="search"]');
    await expect(map).toHaveCount(1);
    await expect(map).toBeHidden();
    await page.waitForTimeout(500);
    expect(requests).toEqual([]);

    await page.getByRole('link', { name: 'Map', exact: true }).click();
    await expect(page).toHaveURL(/view=map/);
    await expect(map).toBeVisible();
    await expect(map).toHaveAttribute('data-map-state', 'ready', { timeout: 30000 });
    const mapped = await page.locator('#listings-grid [data-map-lat]').count();
    expect(mapped).toBeGreaterThan(0);
    await expect(map.locator('[data-map-badge] button')).toHaveCount(mapped);
    expect(failed).toEqual([]);
  });

  test('a badge opens the listing card, whose link keeps the search', async ({ page }) => {
    const search = 'check_in=2030-08-01&check_out=2030-08-04&guests=2';
    await page.goto(`/listings?${search}`);
    const map = page.locator('[data-map="search"]');
    await expect(map).toHaveAttribute('data-map-state', 'ready', { timeout: 30000 });
    const target = KNOWN_LISTINGS.courtyardStudio;
    await map.getByRole('button', { name: new RegExp(`^${target.name}, `) }).click();
    const link = page.locator('.maplibregl-popup a').first();
    await expect(link).toBeVisible();
    await expect(link).toHaveAttribute('href', `/listings/${target.slug}?${search}`);
  });

  test('the listing page shows an area for an approximate listing, never the stored point', async ({ page }) => {
    const target = KNOWN_LISTINGS.courtyardStudio;
    const response = await page.goto(`/listings/${target.slug}`);
    const html = (await response?.text()) ?? '';
    for (const stored of [KNOWN_LOCATION.latitude, KNOWN_LOCATION.longitude]) {
      expect(html).not.toContain(String(stored));
    }
    const map = page.locator('#location [data-map="listing"]');
    await expect(map).toHaveAttribute('data-precision', 'approximate');
    await expect(page.locator('#location')).toContainText('The exact address is shared after you book.');
    await map.scrollIntoViewIfNeeded();
    await expect(map).toHaveAttribute('data-map-state', 'ready', { timeout: 30000 });
  });

  test('an exact listing shows its pin and says so', async ({ page }) => {
    const copy = createListingCopy();
    psql(`UPDATE listing SET latitude = ${EXACT_LOCATION.latitude}, longitude = ${EXACT_LOCATION.longitude},
      location_privacy = 'exact' WHERE id = '${copy.id}';`);
    await page.goto(`/listings/${copy.slug}`);
    const map = page.locator('#location [data-map="listing"]');
    await expect(map).toHaveAttribute('data-precision', 'exact');
    await expect(map).toHaveAttribute('data-lat', String(EXACT_LOCATION.latitude));
    await expect(page.locator('#location')).toContainText('Exact location shown.');
    await map.scrollIntoViewIfNeeded();
    await expect(map).toHaveAttribute('data-map-state', 'ready', { timeout: 30000 });
    await expect(map.locator('.maplibregl-marker')).toHaveCount(1);
  });
});
