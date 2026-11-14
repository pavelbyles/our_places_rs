import { test, expect, type Page } from '@playwright/test';
import { open, PAGE_CSS, SWIFTSHADER, TILES, type Setup } from './map-harness';

/**
 * Spec 103 §1.8: several booking cards can each hold a map. Each builds only
 * near the viewport, and one more than two viewports away is removed, so
 * WebGL contexts stay capped however the guest scrolls.
 */

const POINTS = [
  [18.47127, -77.91861],
  [18.4074, -77.1031],
  [18.012345, -76.790123],
];

function page_html(): string {
  const maps = POINTS.map(
    ([lat, lng], i) => `<article class="card">
  <div id="map-${i}" data-map="booking"
    data-map-js="/assets/maplibre.mjs"
    data-map-worker="/assets/maplibre-worker.mjs"
    data-map-css="/assets/maplibre.css"
    data-pmtiles-js="/assets/pmtiles.js"
    data-map-init="/assets/map.js"
    data-tiles-base="${TILES}"
    data-lat="${lat}" data-lng="${lng}" data-precision="exact">
    <div data-map-canvas=""></div>
    <p data-map-fallback="" hidden="">Map unavailable</p>
  </div>
  <p>Exact location for your stay.</p>
</article>
<div class="spacer"></div>`,
  ).join('\n');
  return `<!doctype html><html><head>
<link rel="stylesheet" href="/assets/page.css">
<script src="/assets/ui.js" defer></script>
</head><body>
<div class="spacer"></div>
${maps}
</body></html>`;
}

const live = (page: Page) => page.locator('canvas.maplibregl-canvas');

/** Maps with a live MapLibre instance. */
function liveMaps(page: Page): Promise<string[]> {
  return page.evaluate(() =>
    [...document.querySelectorAll('[data-map]')]
      .filter((el) => (el as unknown as { __opMap?: unknown }).__opMap)
      .map((el) => el.id),
  );
}

test.use(SWIFTSHADER);

test.describe('booking maps', () => {
  test('each map builds near the viewport and at most two are live', async ({ page }) => {
    const setup: Setup = { mapRequests: [], css: PAGE_CSS };
    await open(page, page_html(), setup);
    await page.waitForTimeout(300);
    expect(await liveMaps(page)).toEqual([]);

    for (let i = 0; i < POINTS.length; i++) {
      const map = page.locator(`#map-${i}`);
      await map.scrollIntoViewIfNeeded();
      await expect(map).toHaveAttribute('data-map-state', 'ready', { timeout: 20000 });
      await expect(map.locator('.maplibregl-marker')).toHaveCount(1);
      const now = await liveMaps(page);
      expect(now).toContain(`map-${i}`);
      expect(now.length).toBeLessThanOrEqual(2);
      expect(await live(page).count()).toBe(now.length);
    }
    // The first map is far away by now: removed, and idle until it returns.
    await expect(page.locator('#map-0')).toHaveAttribute('data-map-state', 'idle');
    await page.locator('#map-0').scrollIntoViewIfNeeded();
    await expect(page.locator('#map-0')).toHaveAttribute('data-map-state', 'ready', { timeout: 20000 });
  });

  test('fast scrolling leaves no stray maps or contexts', async ({ page }) => {
    const setup: Setup = { mapRequests: [], css: PAGE_CSS };
    await open(page, page_html(), setup);
    for (let round = 0; round < 4; round++) {
      for (let i = 0; i < POINTS.length; i++) {
        await page.locator(`#map-${i}`).scrollIntoViewIfNeeded();
        await page.waitForTimeout(30);
      }
      await page.evaluate(() => window.scrollTo(0, 0));
      await page.waitForTimeout(30);
    }
    await page.locator('#map-1').scrollIntoViewIfNeeded();
    await expect(page.locator('#map-1')).toHaveAttribute('data-map-state', 'ready', { timeout: 20000 });
    await page.waitForTimeout(1500);
    const now = await liveMaps(page);
    expect(now).toContain('map-1');
    expect(now.length).toBeLessThanOrEqual(2);
    expect(await live(page).count()).toBe(now.length);
    await expect(page.locator('#map-1 canvas.maplibregl-canvas')).toHaveCount(1);
  });
});
