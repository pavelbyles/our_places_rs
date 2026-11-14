import { test, expect, type Page } from '@playwright/test';
import { open, PAGE_CSS, SWIFTSHADER, TILES, type Setup } from './map-harness';

/**
 * Spec 103 §1.9: the admin pin picker. The Latitude and Longitude inputs
 * stay the source of truth: clicks and drags write them to 6 decimal
 * places, and a typed value moves the pin.
 */

function page_html(lat = '', lng = ''): string {
  const point = lat && lng ? ` data-lat="${lat}" data-lng="${lng}" data-precision="exact"` : '';
  return `<!doctype html><html><head>
<link rel="stylesheet" href="/assets/page.css">
<script src="/assets/ui.js" defer></script>
</head><body>
<form id="listing-form" method="post" action="/listings/new">
  <label>Latitude <input id="lat" name="latitude" inputmode="decimal" value="${lat}"></label>
  <label>Longitude <input id="lng" name="longitude" inputmode="decimal" value="${lng}"></label>
  <div id="map" data-map="picker"
    data-map-js="/assets/maplibre.mjs"
    data-map-worker="/assets/maplibre-worker.mjs"
    data-map-css="/assets/maplibre.css"
    data-pmtiles-js="/assets/pmtiles.js"
    data-map-init="/assets/map.js"
    data-tiles-base="${TILES}"${point}>
    <div data-map-canvas=""></div>
    <p data-map-fallback="" hidden="">Map unavailable</p>
  </div>
  <button id="save">Save</button>
</form>
</body></html>`;
}

const CSS = `${PAGE_CSS}
  [data-map] { height: 360px; }
`;

const map = (page: Page) => page.locator('#map');
const marker = (page: Page) => page.locator('#map .maplibregl-marker');
const SIX_DP = /^-?\d+\.\d{6}$/;

async function ready(page: Page): Promise<void> {
  await expect(map(page)).toHaveAttribute('data-map-state', 'ready', { timeout: 20000 });
}

/** Where the marker's point is, read back through the map. */
function markerAt(page: Page): Promise<{ lat: number; lng: number }> {
  return page.evaluate(() => {
    const el = document.getElementById('map') as unknown as {
      __opMap: { unproject(p: [number, number]): { lat: number; lng: number } };
    } & HTMLElement;
    const canvas = el.querySelector('canvas') as HTMLCanvasElement;
    const c = canvas.getBoundingClientRect();
    const m = (el.querySelector('.maplibregl-marker') as HTMLElement).getBoundingClientRect();
    // The default pin's tip is its bottom centre.
    const p = el.__opMap.unproject([m.left + m.width / 2 - c.left, m.bottom - c.top]);
    return { lat: p.lat, lng: p.lng };
  });
}

test.use(SWIFTSHADER);

test.describe('pin picker', () => {
  test('an empty form has no pin until the first click, which fills both inputs', async ({ page }) => {
    const setup: Setup = { mapRequests: [], css: CSS };
    const refused = await open(page, page_html(), setup);
    await ready(page);
    await expect(marker(page)).toHaveCount(0);
    await expect(page.locator('#map [data-map-overlay]')).toHaveCount(0);

    const box = await map(page).boundingBox();
    if (!box) throw new Error('no map box');
    await page.mouse.click(box.x + box.width / 2, box.y + box.height / 2);
    await expect(marker(page)).toHaveCount(1);
    const lat = await page.locator('#lat').inputValue();
    const lng = await page.locator('#lng').inputValue();
    expect(lat).toMatch(SIX_DP);
    expect(lng).toMatch(SIX_DP);
    expect(Number(lat)).toBeGreaterThan(17.55);
    expect(Number(lat)).toBeLessThan(18.7);
    expect(Number(lng)).toBeGreaterThan(-78.6);
    expect(Number(lng)).toBeLessThan(-75.95);
    expect(refused).toEqual([]);
  });

  test('dragging the pin rewrites both inputs to 6 decimal places', async ({ page }) => {
    await open(page, page_html('18.012345', '-76.790123'), { mapRequests: [], css: CSS });
    await ready(page);
    await expect(marker(page)).toHaveCount(1);
    const m = await marker(page).boundingBox();
    if (!m) throw new Error('no marker');
    const x = m.x + m.width / 2;
    const y = m.y + m.height / 2;
    await page.mouse.move(x, y);
    await page.mouse.down();
    await page.mouse.move(x + 40, y + 20, { steps: 8 });
    await page.mouse.move(x + 80, y + 40, { steps: 8 });
    await page.mouse.up();
    await expect(page.locator('#lat')).not.toHaveValue('18.012345');
    const lat = await page.locator('#lat').inputValue();
    const lng = await page.locator('#lng').inputValue();
    expect(lat).toMatch(SIX_DP);
    expect(lng).toMatch(SIX_DP);
    // East and south of where it was.
    expect(Number(lng)).toBeGreaterThan(-76.790123);
    expect(Number(lat)).toBeLessThan(18.012345);
    const at = await markerAt(page);
    expect(at.lat).toBeCloseTo(Number(lat), 3);
    expect(at.lng).toBeCloseTo(Number(lng), 3);
  });

  test('a typed point moves the pin; one outside Jamaica says so', async ({ page }) => {
    await open(page, page_html('18.012345', '-76.790123'), { mapRequests: [], css: CSS });
    await ready(page);
    await page.locator('#lat').fill('18.020000');
    await page.locator('#lng').fill('-76.780000');
    await page.locator('#lng').press('Tab');
    await expect
      .poll(async () => {
        const at = await markerAt(page);
        return Math.abs(at.lat - 18.02) < 0.002 && Math.abs(at.lng + 76.78) < 0.002;
      }, { timeout: 10000 })
      .toBe(true);
    await expect(page.locator('#map [data-map-overlay]')).toHaveCount(0);

    await page.locator('#lat').fill('40.7128');
    await page.locator('#lng').fill('-74.0060');
    await page.locator('#lng').press('Tab');
    await expect(page.locator('#map [data-map-overlay]')).toHaveText('Outside the mapped area');
    // The form keeps what was typed.
    await expect(page.locator('#lat')).toHaveValue('40.7128');

    // Back inside: the note goes.
    await page.locator('#lat').fill('18.03');
    await page.locator('#lng').fill('-76.77');
    await page.locator('#lng').press('Tab');
    await expect(page.locator('#map [data-map-overlay]')).toHaveCount(0);
  });

  test('a stored point outside Jamaica starts with the note and no pin', async ({ page }) => {
    await open(page, page_html('40.7128', '-74.006'), { mapRequests: [], css: CSS });
    await ready(page);
    await expect(page.locator('#map [data-map-overlay]')).toHaveText('Outside the mapped area');
    await expect(marker(page)).toHaveCount(0);
  });
});
