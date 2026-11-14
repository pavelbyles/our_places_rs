import { test, expect, type BrowserContext, type Page, type Route } from '@playwright/test';
import * as fs from 'node:fs';
import * as path from 'node:path';

/**
 * Spec 103 §1.11: ui.js's lazy loader and map.js, on a fake-origin page
 * served with the portals' CSP (T13, map origin set) and the T12 fixture
 * served with real 206 range slicing.
 */

const ORIGIN = 'https://portal.test';
const TILES =
  'https://storage.googleapis.com/our-places-gcs-map-tiles-test/v20261113';
const BUCKET_PATH = 'https://storage.googleapis.com/our-places-gcs-map-tiles-test/';
const CSP = [
  "default-src 'self'",
  "script-src 'self'",
  "style-src 'self'",
  "img-src 'self' https://storage.googleapis.com data:",
  "font-src 'self'",
  `connect-src 'self' ${BUCKET_PATH}`,
  "frame-ancestors 'none'",
  "base-uri 'self'",
  "form-action 'self'",
  "object-src 'none'",
  "worker-src 'self'",
].join('; ');

const ASSETS_DIR = path.resolve(__dirname, '../../../web_ui_common/src/assets');
const FIXTURE_DIR = path.resolve(__dirname, 'fixtures/map');
const ASSETS: Record<string, [string, string]> = {
  '/assets/ui.js': ['ui.js', 'text/javascript'],
  '/assets/map.js': ['map.js', 'text/javascript'],
  '/assets/pmtiles.js': ['pmtiles-4.5.0.js', 'text/javascript'],
  '/assets/maplibre.mjs': ['maplibre-gl-6.15.0.mjs', 'application/javascript'],
  '/assets/maplibre-worker.mjs': ['maplibre-gl-worker-6.15.0.mjs', 'application/javascript'],
  '/assets/maplibre.css': ['maplibre-gl-6.15.0.css', 'text/css'],
};

const PAGE_CSS = `
  body { margin: 0; font-family: sans-serif; }
  .spacer { height: 3000px; }
  [data-map] { position: relative; width: 640px; height: 224px; }
  [data-map-canvas] { position: absolute; inset: 0; }
  [hidden] { display: none !important; }
`;

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

/** 206 slicing for a single `bytes=a-b` range, as Cloud Storage does. */
function fulfillFile(route: Route, file: string, type: string): Promise<void> {
  const body = fs.readFileSync(file);
  const range = route.request().headers()['range'];
  const cors = { 'access-control-allow-origin': '*' };
  const match = range && /^bytes=(\d+)-(\d*)$/.exec(range);
  if (match) {
    const start = Number(match[1]);
    const end = Math.min(match[2] ? Number(match[2]) : body.length - 1, body.length - 1);
    return route.fulfill({
      status: 206,
      contentType: type,
      headers: { ...cors, 'content-range': `bytes ${start}-${end}/${body.length}` },
      body: body.subarray(start, end + 1),
    });
  }
  return route.fulfill({ status: 200, contentType: type, headers: cors, body });
}

interface Setup {
  /** Every tile, style, glyph or map script request, in order. */
  mapRequests: string[];
  /** Overrides for tile-bucket paths (relative to TILES). */
  bucket?: Record<string, (route: Route) => Promise<void> | void>;
}

async function serve(
  context: BrowserContext,
  html: string,
  setup: Setup,
): Promise<void> {
  await context.route(`${ORIGIN}/**`, (route) => {
    const url = new URL(route.request().url());
    if (url.pathname === '/') {
      return route.fulfill({
        status: 200,
        contentType: 'text/html',
        headers: { 'content-security-policy': CSP },
        body: html,
      });
    }
    if (url.pathname === '/assets/page.css') {
      return route.fulfill({ status: 200, contentType: 'text/css', body: PAGE_CSS });
    }
    const asset = ASSETS[url.pathname];
    if (!asset) return route.fulfill({ status: 404, body: '' });
    if (url.pathname !== '/assets/ui.js') setup.mapRequests.push(url.pathname);
    return route.fulfill({
      status: 200,
      contentType: asset[1],
      headers: { 'content-security-policy': CSP },
      body: fs.readFileSync(path.join(ASSETS_DIR, asset[0])),
    });
  });
  await context.route(`${TILES}/**`, (route) => {
    const rel = new URL(route.request().url()).pathname.replace(
      '/our-places-gcs-map-tiles-test/v20261113/',
      '',
    );
    setup.mapRequests.push(rel);
    const override = setup.bucket?.[rel];
    if (override) return override(route);
    const fontRange = /^fonts\/([^/]+)\/(\d+-\d+)\.pbf$/.exec(decodeURIComponent(rel));
    if (fontRange) {
      // The fixture ships only 0-255; other ranges are empty glyph sets.
      const file = path.join(FIXTURE_DIR, 'fonts', fontRange[1], `${fontRange[2]}.pbf`);
      if (!fs.existsSync(file)) {
        return route.fulfill({
          status: 200,
          contentType: 'application/x-protobuf',
          headers: { 'access-control-allow-origin': '*' },
          body: Buffer.alloc(0),
        });
      }
      return fulfillFile(route, file, 'application/x-protobuf');
    }
    const file = path.join(FIXTURE_DIR, decodeURIComponent(rel));
    if (!fs.existsSync(file)) return route.fulfill({ status: 404, body: '' });
    const type = rel.endsWith('.json')
      ? 'application/json'
      : rel.endsWith('.png')
        ? 'image/png'
        : 'application/octet-stream';
    return fulfillFile(route, file, type);
  });
}

async function open(page: Page, html: string, setup: Setup): Promise<string[]> {
  const violations: string[] = [];
  page.on('console', (msg) => {
    if (msg.text().includes('Refused to')) violations.push(msg.text());
  });
  await serve(page.context(), html, setup);
  await page.goto(`${ORIGIN}/`);
  await page.evaluate(() => {
    document.addEventListener('securitypolicyviolation', (e) => {
      (window as unknown as { __csp: string[] }).__csp ??= [];
      (window as unknown as { __csp: string[] }).__csp.push(
        `${e.violatedDirective} ${e.blockedURI}`,
      );
    });
  });
  return violations;
}

const map = (page: Page) => page.locator('#map');
const unavailable = (page: Page) => page.locator('#map [data-map-fallback]');

test.use({
  launchOptions: {
    args: ['--enable-unsafe-swiftshader', '--use-angle=swiftshader', '--ignore-gpu-blocklist'],
  },
});

test.describe('map.js', () => {
  test('loads nothing until scrolled near, then renders under the CSP', async ({ page }) => {
    const setup: Setup = { mapRequests: [] };
    const refused = await open(page, page_html(), setup);
    await page.waitForTimeout(500);
    expect(setup.mapRequests).toEqual([]);

    await map(page).scrollIntoViewIfNeeded();
    await expect(map(page)).toHaveAttribute('data-map-state', 'ready', { timeout: 20000 });
    await expect(page.locator('#map canvas.maplibregl-canvas')).toHaveCount(1);
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
