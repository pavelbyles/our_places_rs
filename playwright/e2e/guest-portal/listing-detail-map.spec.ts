import { test, expect } from '../fixtures/base.fixture.js';
import { EXACT_LOCATION, KNOWN_LISTINGS, KNOWN_LOCATION } from '../fixtures/test-data.js';
import { createListingCopy, psql } from '../fixtures/bookings-data.js';
import { SWIFTSHADER, watchMapRequests } from '../fixtures/map-watch.js';

/** Spec 103 §1.7 on the running guest portal (see listings-map.spec.ts). */

test.use(SWIFTSHADER);

const target = KNOWN_LISTINGS.courtyardStudio;

test.describe('Guest Portal - listing page map', () => {
  test('approximate: an area loads only after scrolling, never the stored point', async ({ page }) => {
    const watch = watchMapRequests(page);
    const response = await page.goto(`/listings/${target.slug}`);
    const html = (await response?.text()) ?? '';
    for (const stored of [KNOWN_LOCATION.latitude, KNOWN_LOCATION.longitude]) {
      expect(html).not.toContain(String(stored));
    }
    const map = page.locator('#location [data-map="listing"]');
    await expect(map).toHaveAttribute('data-precision', 'approximate');
    await expect(page.locator('#location')).toContainText('The exact address is shared after you book.');
    await page.waitForTimeout(500);
    expect(watch.requests).toEqual([]);

    await map.scrollIntoViewIfNeeded();
    await expect(map).toHaveAttribute('data-map-state', 'ready', { timeout: 30000 });
    const drawn = await map.evaluate((el) => {
      const m = (el as unknown as { __opMap: { getLayer(id: string): unknown } }).__opMap;
      return { circle: !!m.getLayer('op-area-fill'), pins: el.querySelectorAll('.maplibregl-marker').length };
    });
    expect(drawn).toEqual({ circle: true, pins: 0 });
    expect(watch.failed).toEqual([]);
  });

  test('exact: a pin, and the page says so', async ({ page }) => {
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

test.describe('Guest Portal - listing page map without JavaScript', () => {
  test.use({ javaScriptEnabled: false });

  test('the location text stays and the map says it needs JavaScript', async ({ page }) => {
    await page.goto(`/listings/${target.slug}`);
    await expect(page.locator('#location-heading')).toHaveText(`${target.city}, Jamaica`);
    await expect(page.locator('#location')).toContainText('Map needs JavaScript');
  });
});
