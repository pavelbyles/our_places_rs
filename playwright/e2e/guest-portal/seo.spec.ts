import * as fs from 'node:fs';
import * as path from 'node:path';
import { validateXML } from 'xmllint-wasm';
import { test, expect } from '../fixtures/base.fixture.js';
import {
  createBooking,
  createGuest,
  createListingCopy,
  deactivateListing,
  psql,
} from '../fixtures/bookings-data.js';
import { KNOWN_LISTINGS } from '../fixtures/test-data.js';

// Spec 102: sitemap, robots.txt, head metadata and listing JSON-LD on the
// guest portal. Locally the site is not indexable (SEO_INDEXABLE unset); the
// indexable robots.txt is covered by unit tests and a manual prod check.
const NEW_ONLY = { tag: '@new-only' };
const ORIGIN = (process.env.GUEST_PORTAL_URL || 'http://localhost:3010').replace(/\/$/, '');
const SITEMAP_XSD = fs.readFileSync(path.resolve(__dirname, '../fixtures/sitemap-0.9.xsd'), 'utf8');
const STATIC_PATHS = ['/', '/listings', '/about', '/terms', '/cancellation-policy', '/privacy-policy', '/house-rules'];

function locs(xml: string): string[] {
  return [...xml.matchAll(/<loc>([^<]*)<\/loc>/g)].map((m) => m[1].replace(/&amp;/g, '&'));
}

/** A listing copy only this test uses; other specs change listings in parallel. */
function softDeletedListing(): { slug: string } {
  const copy = createListingCopy();
  psql(`UPDATE listing SET deleted_at = now() WHERE id = '${copy.id}';`);
  return copy;
}

/** The page's single JSON-LD block, parsed. */
async function structuredData(page: import('@playwright/test').Page): Promise<Record<string, any>> {
  const blocks = page.locator('script[type="application/ld+json"]');
  await expect(blocks).toHaveCount(1);
  const text = (await blocks.first().textContent()) ?? '';
  return JSON.parse(text);
}

test.describe('Guest Portal - SEO (spec 102)', () => {
  test('sitemap lists public pages and live listings only', NEW_ONLY, async ({ request }) => {
    const live = createListingCopy();
    const dormant = createListingCopy();
    deactivateListing(dormant.id);
    const gone = softDeletedListing();

    const res = await request.get('/sitemap.xml');
    expect(res.status()).toBe(200);
    expect(res.headers()['content-type']).toContain('application/xml');
    expect(res.headers()['cache-control']).toBe('public, max-age=3600');
    const xml = await res.text();
    expect(xml).toContain('<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">');

    const found = locs(xml);
    for (const p of STATIC_PATHS) {
      expect(found, p).toContain(`${ORIGIN}${p}`);
    }
    for (const loc of found) {
      expect(loc.startsWith(`${ORIGIN}/`), loc).toBe(true);
    }
    expect(found).toContain(`${ORIGIN}/listings/${live.slug}`);
    expect(found).not.toContain(`${ORIGIN}/listings/${dormant.slug}`);
    expect(found).not.toContain(`${ORIGIN}/listings/${gone.slug}`);

    for (const lastmod of xml.matchAll(/<lastmod>([^<]*)<\/lastmod>/g)) {
      expect(Number.isNaN(Date.parse(lastmod[1])), lastmod[1]).toBe(false);
    }
    expect(xml).toMatch(new RegExp(`<loc>${ORIGIN}/listings/${live.slug}</loc><lastmod>`));
  });

  test('sitemap validates against the sitemaps.org 0.9 schema', NEW_ONLY, async ({ request }) => {
    const xml = await (await request.get('/sitemap.xml')).text();
    const result = await validateXML({
      xml: [{ fileName: 'sitemap.xml', contents: xml }],
      schema: [{ fileName: 'sitemap-0.9.xsd', contents: SITEMAP_XSD }],
    });
    expect(result.errors.map((e) => e.rawMessage)).toEqual([]);
    expect(result.valid).toBe(true);
  });

  test('robots.txt keeps a non-indexable site out entirely', NEW_ONLY, async ({ request }) => {
    const res = await request.get('/robots.txt');
    expect(res.status()).toBe(200);
    expect(res.headers()['content-type']).toContain('text/plain');
    expect(await res.text()).toBe('User-agent: *\nDisallow: /\n');
    expect(res.headers()['x-robots-tag']).toBe('noindex, nofollow');
  });

  test('every response of a non-indexable site is noindex', NEW_ONLY, async ({ request }) => {
    for (const p of ['/', '/listings', '/sitemap.xml', '/no-such-page']) {
      const res = await request.get(p);
      expect(res.headers()['x-robots-tag'], p).toBe('noindex, nofollow');
    }
  });

  test('listing pages carry one VacationRental JSON-LD block', NEW_ONLY, async ({ page, request }) => {
    const listing = createListingCopy();
    for (const stars of [5, 4, 5]) {
      const guest = createGuest();
      const booking = await createBooking(request, guest.id, { listingId: listing.id });
      psql(`INSERT INTO review (booking_id, listing_id, guest_id, cleanliness_rating, accuracy_rating,
                                location_rating, value_rating, overall_rating)
            VALUES ('${booking.id}', '${listing.id}', '${guest.id}', ${stars}, ${stars}, ${stars}, ${stars}, ${stars});`);
    }

    const res = await page.goto(`/listings/${listing.slug}`);
    expect(res?.status()).toBe(200);
    const ld = await structuredData(page);
    expect(ld['@context']).toBe('https://schema.org');
    expect(ld['@type']).toBe('VacationRental');
    expect(ld.name).toBe(listing.name);
    expect(ld.identifier).toBe(listing.id);
    expect(ld.url).toBe(`${ORIGIN}/listings/${listing.slug}`);
    expect(await page.locator('link[rel="canonical"]').getAttribute('href')).toBe(ld.url);
    expect(ld.address['@type']).toBe('PostalAddress');
    expect(ld.address.addressCountry).toBeTruthy();
    expect(ld.address.streetAddress).toBeUndefined();
    expect(ld.containsPlace.occupancy.value).toBeGreaterThan(0);
    expect(ld.offers).toBeUndefined();
    expect(ld.makesOffer.priceSpecification.price).toMatch(/^\d+(\.\d+)?$/);
    expect(ld.makesOffer.priceSpecification.priceCurrency).toMatch(/^[A-Z]{3}$/);
    expect(ld.aggregateRating.reviewCount).toBe(3);
    expect(ld.aggregateRating.ratingValue).toBeCloseTo(4.67, 2);
  });

  test('head tags give titles, absolute canonicals and previews', NEW_ONLY, async ({ page }) => {
    const villa = KNOWN_LISTINGS.courtyardStudio;
    await page.goto(`/listings/${villa.slug}?check_in=2032-03-01&check_out=2032-03-04&currency=EUR`);
    await expect(page).toHaveTitle(`${villa.name} · Our Places`);
    const head = async (selector: string, attr = 'content') =>
      page.locator(selector).first().getAttribute(attr);
    expect(await head('link[rel="canonical"]', 'href')).toBe(`${ORIGIN}/listings/${villa.slug}`);
    expect(await head('meta[property="og:url"]')).toBe(`${ORIGIN}/listings/${villa.slug}`);
    expect(await head('meta[property="og:title"]')).toBe(`${villa.name} · Our Places`);
    expect(await head('meta[property="og:site_name"]')).toBe('Our Places');
    expect(await head('meta[name="description"]')).toBeTruthy();
    expect(await head('meta[property="og:description"]')).toBeTruthy();
    expect(await head('meta[property="og:image"]')).toMatch(/^https?:\/\//);
    expect(await head('meta[name="twitter:card"]')).toBe('summary_large_image');
    await expect(page.locator('meta[charset]')).toHaveCount(1);
  });

  test('a listing reached by its id declares the slug canonical', NEW_ONLY, async ({ page }) => {
    const listing = createListingCopy();
    await page.goto(`/listings/${listing.id}`);
    expect(await page.locator('link[rel="canonical"]').getAttribute('href')).toBe(
      `${ORIGIN}/listings/${listing.slug}`,
    );
    const ld = await structuredData(page);
    expect(ld.url).toBe(`${ORIGIN}/listings/${listing.slug}`);
  });

  test('the home page uses the site defaults and the default preview image', NEW_ONLY, async ({ page }) => {
    await page.goto('/');
    await expect(page).toHaveTitle('Luxury Jamaica Stays · Our Places');
    expect(await page.locator('link[rel="canonical"]').getAttribute('href')).toBe(`${ORIGIN}/`);
    const image = await page.locator('meta[property="og:image"]').getAttribute('content');
    expect(image).toMatch(new RegExp(`^${ORIGIN}/.*og-default.*\\.jpg$`));
    expect(await page.locator('meta[property="og:image:width"]').getAttribute('content')).toBe('1200');
    expect(await page.locator('meta[property="og:image:height"]').getAttribute('content')).toBe('630');
    await expect(page.locator('script[type="application/ld+json"]')).toHaveCount(0);
  });

  test('static pages have their own titles', NEW_ONLY, async ({ page }) => {
    for (const [p, title] of [
      ['/listings', 'Villas in Jamaica'],
      ['/about', 'About Our Places'],
      ['/terms', 'Terms and conditions'],
      ['/cancellation-policy', 'Cancellation policy'],
      ['/privacy-policy', 'Privacy policy'],
      ['/house-rules', 'House rules'],
      ['/login', 'Sign in'],
      ['/register', 'Register'],
    ]) {
      await page.goto(p);
      await expect(page, p).toHaveTitle(`${title} · Our Places`);
      expect(await page.locator('link[rel="canonical"]').getAttribute('href'), p).toBe(`${ORIGIN}${p}`);
    }
  });

  test('an unknown listing is a 404 with no structured data', NEW_ONLY, async ({ page }) => {
    const res = await page.goto('/listings/no-such-villa-e2e');
    expect(res?.status()).toBe(404);
    await expect(page.locator('script[type="application/ld+json"]')).toHaveCount(0);
  });
});
