import type { BrowserContext, Page, Route } from '@playwright/test';
import * as fs from 'node:fs';
import * as path from 'node:path';

/**
 * Shared by the ui-js map specs (spec 103 §1.11): a fake-origin page served
 * with the portals' CSP (T13, map origin set), the vendored map assets, and
 * the T12 fixture served with real 206 range slicing.
 */

export const ORIGIN = 'https://portal.test';
export const TILES =
  'https://storage.googleapis.com/our-places-gcs-map-tiles-test/v20261113';
const BUCKET_PATH = 'https://storage.googleapis.com/our-places-gcs-map-tiles-test/';
export const CSP = [
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

export const PAGE_CSS = `
  body { margin: 0; font-family: sans-serif; }
  .spacer { height: 3000px; }
  [data-map] { position: relative; width: 640px; height: 224px; }
  [data-map-canvas] { width: 100%; height: 100%; }
  [hidden] { display: none !important; }
`;

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

export interface Setup {
  /** Every tile, style, glyph or map script request, in order. */
  mapRequests: string[];
  /** Overrides for tile-bucket paths (relative to TILES). */
  bucket?: Record<string, (route: Route) => Promise<void> | void>;
  /** The page stylesheet; the single-map page's by default. */
  css?: string;
}

/** Serves `rel` (relative to TILES) from the T12 fixture. */
export function fixture(route: Route, rel: string): Promise<void> {
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
}

export async function serve(
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
      return route.fulfill({ status: 200, contentType: 'text/css', body: setup.css ?? PAGE_CSS });
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
    return fixture(route, rel);
  });
}

export async function open(page: Page, html: string, setup: Setup): Promise<string[]> {
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

/** Software WebGL, so maps render headless. */
export const SWIFTSHADER = {
  launchOptions: {
    args: ['--enable-unsafe-swiftshader', '--use-angle=swiftshader', '--ignore-gpu-blocklist'],
  },
};
