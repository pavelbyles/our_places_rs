import { type Page } from '@playwright/test';

/** URLs that belong to the map: scripts, styles, tiles, fonts and sprites. */
export const MAP_URL = /maplibre|pmtiles|\/map-[0-9a-f]+\.js|map-tiles|\.pmtiles|style-(light|dark)\.json/;

/** Records map requests and any that fail (network error or HTTP >= 400). */
export function watchMapRequests(page: Page): { requests: string[]; failed: string[] } {
  const requests: string[] = [];
  const failed: string[] = [];
  page.on('request', (r) => {
    if (MAP_URL.test(r.url())) requests.push(r.url());
  });
  page.on('requestfailed', (r) => {
    if (MAP_URL.test(r.url())) failed.push(`${r.url()} ${r.failure()?.errorText}`);
  });
  page.on('response', (r) => {
    if (MAP_URL.test(r.url()) && r.status() >= 400) failed.push(`${r.url()} ${r.status()}`);
  });
  return { requests, failed };
}

/** Software WebGL, so maps render headless. */
export const SWIFTSHADER = {
  launchOptions: {
    args: ['--enable-unsafe-swiftshader', '--use-angle=swiftshader', '--ignore-gpu-blocklist'],
  },
};
