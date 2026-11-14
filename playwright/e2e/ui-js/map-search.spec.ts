import { test, expect, type Page } from '@playwright/test';
import { fixture, open, PAGE_CSS, SWIFTSHADER, TILES, type Setup } from './map-harness';

/**
 * Spec 103 §1.6: the /listings search map. Badges come from the result
 * cards on the page, approximate listings get circles, popups are cloned
 * from server-rendered templates, and the phone List/Map toggle switches in
 * place.
 */

interface Card {
  slug: string;
  name: string;
  price: string;
  lat?: number;
  lng?: number;
  precision?: 'approximate' | 'exact';
}

const MONTEGO: Card = { slug: 'mo-bay', name: 'Mo Bay Villa', price: 'US$650', lat: 18.47127, lng: -77.91861, precision: 'approximate' };
const OCHO: Card = { slug: 'ocho', name: 'Ocho <Rios> & Co', price: 'US$900', lat: 18.4074, lng: -77.1031, precision: 'exact' };
const NEGRIL: Card = { slug: 'negril', name: 'Negril Cliffs', price: 'US$720', lat: 18.268, lng: -78.348, precision: 'approximate' };
const NOWHERE: Card = { slug: 'nowhere', name: 'Unmapped', price: 'US$500' };
const SEARCH = 'check_in=2030-08-01&amp;check_out=2030-08-04&amp;guests=2';

const escape = (s: string) =>
  s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');

function card(c: Card): string {
  const data =
    c.lat === undefined
      ? ''
      : ` data-map-lat="${c.lat}" data-map-lng="${c.lng}" data-map-precision="${c.precision}" data-map-price="${c.price}" data-map-name="${escape(c.name)}"`;
  return `<div id="listing-card-${c.slug}"${data}>
  <article><h3><a href="/listings/${c.slug}?${SEARCH}">${escape(c.name)}</a></h3></article>
  <template data-map-card="${c.slug}"><a data-popup-link="" href="/listings/${c.slug}?${SEARCH}">${escape(c.name)} ${c.price}</a></template>
</div>`;
}

function page_html(cards: Card[], view: 'list' | 'map' = 'map'): string {
  const onMap = view === 'map';
  return `<!doctype html><html><head>
<link rel="stylesheet" href="/assets/page.css">
<script src="/assets/ui.js" defer></script>
</head><body>
<div data-results="" data-view="${view}">
  <nav data-view-toggle="">
    <a id="to-list" href="/?view=list" data-view-link="list"${onMap ? '' : ' aria-current="page"'}>List</a>
    <a id="to-map" href="/?view=map" data-view-link="map"${onMap ? ' aria-current="page"' : ''}>Map</a>
  </nav>
  <div id="listings-grid" data-results-list="" class="${onMap ? 'hidden md:grid' : 'grid'}">
    ${cards.map(card).join('\n')}
  </div>
  <div id="results-map" data-results-map="" class="${onMap ? 'md:block' : 'hidden md:block'}">
    <div id="map" data-map="search"
      data-map-js="/assets/maplibre.mjs"
      data-map-worker="/assets/maplibre-worker.mjs"
      data-map-css="/assets/maplibre.css"
      data-pmtiles-js="/assets/pmtiles.js"
      data-map-init="/assets/map.js"
      data-tiles-base="${TILES}">
      <div data-map-canvas=""></div>
      <p data-map-fallback="" hidden="">Map unavailable</p>
    </div>
  </div>
</div>
</body></html>`;
}

// No Tailwind here: `.hidden` is the only utility the page needs, so this
// viewport behaves like a phone for the toggle.
const SEARCH_CSS = `${PAGE_CSS}
  .hidden { display: none; }
  [data-map] { height: 480px; }
`;

const map = (page: Page) => page.locator('#map');
const badges = (page: Page) => page.locator('#map [data-map-badge] button');

async function ready(page: Page): Promise<void> {
  await expect(map(page)).toHaveAttribute('data-map-state', 'ready', { timeout: 20000 });
}

/** What the live map holds: circles, zoom and centre. */
function drawn(page: Page) {
  return page.evaluate(() => {
    const el = document.getElementById('map') as unknown as {
      __opMap: {
        getSource(id: string): { _data?: { features?: unknown[] } } | undefined;
        getLayer(id: string): unknown;
        getZoom(): number;
        getCenter(): { lng: number; lat: number };
      };
    };
    const m = el.__opMap;
    const source = m.getSource('op-area') as unknown as { serialize?: () => { data: { features: unknown[] } } } | undefined;
    return {
      circles: source?.serialize ? source.serialize().data.features.length : 0,
      layer: !!m.getLayer('op-area-fill'),
      zoom: m.getZoom(),
      center: m.getCenter(),
    };
  });
}

test.use(SWIFTSHADER);

test.describe('search map', () => {
  test('one badge per mapped card, circles for approximate ones, under the CSP', async ({ page }) => {
    const setup: Setup = { mapRequests: [], css: SEARCH_CSS };
    const refused = await open(page, page_html([MONTEGO, OCHO, NEGRIL, NOWHERE]), setup);
    await ready(page);
    const mapped = await page.locator('[data-map-lat]').count();
    expect(mapped).toBe(3);
    await expect(badges(page)).toHaveCount(mapped);
    await expect(badges(page).first()).toHaveAttribute('type', 'button');
    await expect(page.getByRole('button', { name: 'Ocho <Rios> & Co, US$900' })).toBeVisible();
    const box = await badges(page).first().boundingBox();
    expect(box && box.width >= 44 && box.height >= 44).toBe(true);
    const d = await drawn(page);
    expect(d.circles).toBe(2);
    expect(d.layer).toBe(true);
    expect(d.zoom).toBeLessThanOrEqual(13);
    expect(refused).toEqual([]);
    expect(await page.evaluate(() => (window as unknown as { __csp?: string[] }).__csp ?? [])).toEqual([]);
  });

  test('a theme toggle keeps the badges and brings the circles back', async ({ page }) => {
    await open(page, page_html([MONTEGO, OCHO, NEGRIL]), { mapRequests: [], css: SEARCH_CSS });
    await ready(page);
    await page.evaluate(() => document.documentElement.classList.add('dark'));
    await expect.poll(async () => (await drawn(page)).layer, { timeout: 15000 }).toBe(true);
    await expect(badges(page)).toHaveCount(3);
    expect((await drawn(page)).circles).toBe(2);
  });

  test('a theme toggle before the first load keeps one badge per card', async ({ page }) => {
    let release: () => void = () => undefined;
    const held = new Promise<void>((r) => (release = r));
    const setup: Setup = {
      mapRequests: [],
      css: SEARCH_CSS,
      bucket: {
        'style-light.json': async (route) => {
          await held;
          await fixture(route, 'style-light.json');
        },
      },
    };
    await open(page, page_html([MONTEGO, OCHO, NEGRIL]), setup);
    await expect(map(page)).toHaveAttribute('data-map-state', 'loading', { timeout: 20000 });
    await expect.poll(() => page.evaluate(() => !!(document.getElementById('map') as unknown as { __opMap?: unknown }).__opMap)).toBe(true);
    await page.evaluate(() => document.documentElement.classList.add('dark'));
    release();
    await ready(page);
    await expect(badges(page)).toHaveCount(3);
    await expect.poll(async () => (await drawn(page)).circles, { timeout: 15000 }).toBe(2);
  });

  test('clicking a badge opens its card, whose link carries the search', async ({ page }) => {
    await open(page, page_html([MONTEGO, OCHO]), { mapRequests: [], css: SEARCH_CSS });
    await ready(page);
    await page.getByRole('button', { name: 'Mo Bay Villa, US$650' }).click();
    const link = page.locator('.maplibregl-popup [data-popup-link]');
    await expect(link).toBeVisible();
    await expect(link).toHaveAttribute('href', '/listings/mo-bay?check_in=2030-08-01&check_out=2030-08-04&guests=2');
    await expect(page.locator('.maplibregl-popup')).toHaveCount(1);
    // Another badge replaces the popup.
    await page.getByRole('button', { name: 'Ocho <Rios> & Co, US$900' }).click();
    await expect(page.locator('.maplibregl-popup')).toHaveCount(1);
    await expect(page.locator('.maplibregl-popup [data-popup-link]')).toContainText('Ocho <Rios> & Co');
  });

  test('keyboard: Tab reaches a badge, Enter opens, Escape closes and restores focus', async ({ page }) => {
    await open(page, page_html([MONTEGO]), { mapRequests: [], css: SEARCH_CSS });
    await ready(page);
    const badge = page.getByRole('button', { name: 'Mo Bay Villa, US$650' });
    let reached = false;
    for (let i = 0; i < 30 && !reached; i++) {
      await page.keyboard.press('Tab');
      reached = await badge.evaluate((b) => document.activeElement === b);
    }
    expect(reached).toBe(true);
    await page.keyboard.press('Enter');
    await expect(page.locator('.maplibregl-popup')).toHaveCount(1);
    await page.keyboard.press('Escape');
    await expect(page.locator('.maplibregl-popup')).toHaveCount(0);
    await expect(badge).toBeFocused();
    // Space opens too.
    await page.keyboard.press('Space');
    await expect(page.locator('.maplibregl-popup')).toHaveCount(1);
    await page.locator('.maplibregl-popup [data-popup-link]').focus();
    await page.keyboard.press('Escape');
    await expect(page.locator('.maplibregl-popup')).toHaveCount(0);
    await expect(badge).toBeFocused();
  });

  test('one marker centres the map at zoom 12', async ({ page }) => {
    await open(page, page_html([MONTEGO, NOWHERE]), { mapRequests: [], css: SEARCH_CSS });
    await ready(page);
    const d = await drawn(page);
    expect(d.zoom).toBeCloseTo(12, 1);
    expect(d.center.lat).toBeCloseTo(MONTEGO.lat as number, 3);
    expect(d.center.lng).toBeCloseTo(MONTEGO.lng as number, 3);
    await expect(page.locator('[data-map-overlay]')).toHaveCount(0);
  });

  test('no mapped listings: Jamaica and a note', async ({ page }) => {
    await open(page, page_html([NOWHERE]), { mapRequests: [], css: SEARCH_CSS });
    await ready(page);
    await expect(page.locator('[data-map-overlay]')).toHaveText("These listings aren't on the map yet.");
    await expect(badges(page)).toHaveCount(0);
    const d = await drawn(page);
    expect(d.center.lng).toBeGreaterThan(-78.6);
    expect(d.center.lng).toBeLessThan(-75.95);
  });

  test('the List/Map toggle switches in place and keeps the URL in step', async ({ page }) => {
    const setup: Setup = { mapRequests: [], css: SEARCH_CSS };
    await open(page, page_html([MONTEGO, OCHO], 'list'), setup);
    await expect(page.locator('#results-map')).toBeHidden();
    await page.waitForTimeout(300);
    expect(setup.mapRequests).toEqual([]);

    await page.locator('#to-map').click();
    await expect(page).toHaveURL(/\/\?view=map$/);
    await expect(page.locator('#listings-grid')).toBeHidden();
    await expect(page.locator('#results-map')).toBeVisible();
    await expect(page.locator('#to-map')).toHaveAttribute('aria-current', 'page');
    await expect(page.locator('#to-list')).not.toHaveAttribute('aria-current', 'page');
    await ready(page);
    await expect(badges(page)).toHaveCount(2);

    await page.locator('#to-list').click();
    await expect(page).toHaveURL(/\/\?view=list$/);
    await expect(page.locator('#listings-grid')).toBeVisible();
    await expect(page.locator('#results-map')).toBeHidden();
    await expect(page.locator('#to-list')).toHaveAttribute('aria-current', 'page');
  });
});
