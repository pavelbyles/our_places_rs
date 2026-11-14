import { test, expect, type Page } from '@playwright/test';
import { open, SWIFTSHADER, TILES, type Setup } from './map-harness';

/**
 * Spec 103 §1.11: ui.js's lazy loader and map.js, on a fake-origin page
 * served with the portals' CSP and the T12 fixture (see map-harness.ts).
 */

function page_html(attrs: { kind?: string; precision?: string } = {}): string {
  const kind = attrs.kind ?? 'listing';
  const precision = attrs.precision ?? 'approximate';
  return `<!doctype html><html><head>
<link rel="stylesheet" href="/assets/page.css">
<script src="/assets/ui.js" defer></script>
</head><body>
<div class="spacer" id="top"></div>
<section id="location">
  <p id="location-text">Montego Bay, Jamaica</p>
  <div id="map" data-map="${kind}"
    data-map-js="/assets/maplibre.mjs"
    data-map-worker="/assets/maplibre-worker.mjs"
    data-map-css="/assets/maplibre.css"
    data-pmtiles-js="/assets/pmtiles.js"
    data-map-init="/assets/map.js"
    data-tiles-base="${TILES}"
    data-lat="18.47127" data-lng="-77.91861" data-precision="${precision}">
    <div data-map-canvas=""></div>
    <p data-map-fallback="" hidden="">Map unavailable</p>
  </div>
</section>
<div class="spacer" id="bottom"></div>
</body></html>`;
}

const map = (page: Page) => page.locator('#map');
const unavailable = (page: Page) => page.locator('#map [data-map-fallback]');

test.use(SWIFTSHADER);

test.describe('map.js', () => {
  test('loads nothing until scrolled near, then renders under the CSP', async ({ page }) => {
    const setup: Setup = { mapRequests: [] };
    const refused = await open(page, page_html(), setup);
    await page.waitForTimeout(500);
    expect(setup.mapRequests).toEqual([]);

    await map(page).scrollIntoViewIfNeeded();
    await expect(map(page)).toHaveAttribute('data-map-state', 'ready', { timeout: 20000 });
    await expect(page.locator('#map canvas.maplibregl-canvas')).toHaveCount(1);
    // The map fills its box once MapLibre's stylesheet has loaded.
    const canvas = await page.locator('#map canvas.maplibregl-canvas').boundingBox();
    const box = await map(page).boundingBox();
    expect(canvas?.height).toBeGreaterThan(0);
    expect(canvas?.height).toBeCloseTo(box?.height ?? -1, 0);
    expect(canvas?.width).toBeCloseTo(box?.width ?? -1, 0);
    await expect(page.locator('#map .maplibregl-ctrl-attrib')).toContainText(
      'OpenStreetMap contributors',
    );
    expect(setup.mapRequests).toContain('jamaica.pmtiles');
    expect(setup.mapRequests).toContain('style-light.json');
    expect(refused).toEqual([]);
    expect(await page.evaluate(() => (window as unknown as { __csp?: string[] }).__csp ?? [])).toEqual([]);
    await expect(unavailable(page)).toBeHidden();
  });

  for (const [precision, expected] of [
    ['approximate', { circle: true, pins: 0 }],
    ['exact', { circle: false, pins: 1 }],
  ] as const) {
    test(`${precision} draws ${expected.circle ? 'a circle and no pin' : 'a pin and no circle'}`, async ({ page }) => {
      const setup: Setup = { mapRequests: [] };
      await open(page, page_html({ precision }), setup);
      await map(page).scrollIntoViewIfNeeded();
      await expect(map(page)).toHaveAttribute('data-map-state', 'ready', { timeout: 20000 });
      const drawn = await page.evaluate(() => {
        const el = document.getElementById('map') as unknown as {
          __opMap: { getLayer(id: string): unknown };
        };
        return {
          circle: !!el.__opMap.getLayer('op-area-fill'),
          pins: document.querySelectorAll('#map .maplibregl-marker').length,
        };
      });
      expect(drawn).toEqual(expected);
    });
  }

  test('a 500 on the style shows "Map unavailable"', async ({ page }) => {
    const setup: Setup = {
      mapRequests: [],
      bucket: {
        'style-light.json': (route) =>
          route.fulfill({ status: 500, headers: { 'access-control-allow-origin': '*' }, body: '' }),
      },
    };
    await open(page, page_html(), setup);
    await map(page).scrollIntoViewIfNeeded();
    await expect(unavailable(page)).toBeVisible({ timeout: 15000 });
    await expect(map(page)).toHaveAttribute('data-map-state', 'failed');
    await expect(page.locator('#location-text')).toBeVisible();
  });

  test('no WebGL shows "Map unavailable" and keeps the location text', async ({ page }) => {
    await page.addInitScript(() => {
      HTMLCanvasElement.prototype.getContext = function () {
        return null;
      } as typeof HTMLCanvasElement.prototype.getContext;
    });
    const setup: Setup = { mapRequests: [] };
    await open(page, page_html(), setup);
    await map(page).scrollIntoViewIfNeeded();
    await expect(unavailable(page)).toBeVisible({ timeout: 15000 });
    await expect(page.locator('#location-text')).toHaveText('Montego Bay, Jamaica');
    expect(setup.mapRequests).not.toContain('jamaica.pmtiles');
  });

  test('a style that never answers shows "Map unavailable" within 11 seconds', async ({ page }) => {
    test.setTimeout(30000);
    const setup: Setup = {
      mapRequests: [],
      bucket: { 'style-light.json': () => undefined },
    };
    await open(page, page_html(), setup);
    await map(page).scrollIntoViewIfNeeded();
    const started = Date.now();
    await expect(unavailable(page)).toBeVisible({ timeout: 11000 });
    expect(Date.now() - started).toBeLessThan(11000);
  });

  test('remove during a pending init leaves one live map (generation token)', async ({ page }) => {
    const setup: Setup = { mapRequests: [] };
    await open(page, page_html(), setup);
    await map(page).scrollIntoViewIfNeeded();
    await expect(map(page)).toHaveAttribute('data-map-state', 'ready', { timeout: 20000 });
    // The library is loaded, so each init resolves on a later microtask; a
    // remove() in between must win.
    await page.evaluate(() => {
      const api = (window as unknown as {
        OurPlacesMap: { init(el: Element): void; remove(el: Element): void };
      }).OurPlacesMap;
      const el = document.getElementById('map') as Element;
      api.remove(el);
      api.init(el);
      api.remove(el);
      api.init(el);
      api.remove(el);
      api.init(el);
    });
    await expect(map(page)).toHaveAttribute('data-map-state', 'ready', { timeout: 20000 });
    await page.waitForTimeout(1000);
    await expect(page.locator('#map canvas.maplibregl-canvas')).toHaveCount(1);
  });

  test('scrolling out and back in quickly leaves one live map', async ({ page }) => {
    const setup: Setup = { mapRequests: [] };
    await open(page, page_html(), setup);
    for (let i = 0; i < 3; i++) {
      await map(page).scrollIntoViewIfNeeded();
      await page.evaluate(() => window.scrollTo(0, document.body.scrollHeight));
      await page.waitForTimeout(50);
    }
    await map(page).scrollIntoViewIfNeeded();
    await expect(map(page)).toHaveAttribute('data-map-state', 'ready', { timeout: 20000 });
    await page.waitForTimeout(1000);
    await expect(page.locator('#map canvas.maplibregl-canvas')).toHaveCount(1);
    await expect(page.locator('#map .maplibregl-ctrl-attrib')).toHaveCount(1);
  });
});
