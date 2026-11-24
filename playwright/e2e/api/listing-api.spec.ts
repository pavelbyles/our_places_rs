import { test, expect } from '@playwright/test';
import { execSync } from 'node:child_process';
import { KNOWN_LISTINGS } from '../fixtures/test-data.js';
import { signTestJwt } from '../fixtures/bookings-data.js';

const DB_URL = process.env.DATABASE_URL || 'postgres://postgres:password@localhost:5432/our_places';
// A host who owns no listing: writes to someone else's listing are a 403.
const OTHER_HOST_ID = '01a0bcbd-014d-7062-99b2-6a42d05b9f58';

function psql(sql: string): string {
  return execSync(`psql "${DB_URL}" -v ON_ERROR_STOP=1 -t -A`, { input: sql }).toString().trim();
}

test.describe('Backend API Validation - Listing Service', () => {
  const listingApiUrl = process.env.LISTING_API_URL || 'http://127.0.0.1:8082';
  const targetVilla = KNOWN_LISTINGS.courtyardStudio;

  test('GET /api/v1/listings should return active listings with valid schema', async ({ request }) => {
    // The list is newest first and paged, so listings other tests add can push
    // the seeded villa off page one. Filter by name to find it wherever it sits.
    const response = await request.get(`${listingApiUrl}/api/v1/listings`, {
      params: { name: targetVilla.name },
    });
    expect(response.status()).toBe(200);

    const listings = await response.json();
    expect(Array.isArray(listings)).toBe(true);
    expect(listings.length).toBeGreaterThan(0);

    const villa = listings.find((l: any) => l.slug === targetVilla.slug);
    expect(villa).toBeDefined();
    expect(villa.name).toBe(targetVilla.name);
    expect(villa.price_per_night).toBe(targetVilla.pricePerNight);
    expect(villa.city).toBe(targetVilla.city);
    expect(villa.country).toBe(targetVilla.country);
    expect(villa.is_active).toBe(true);
    expect(villa.primary_image_url).toBeTruthy();
  });

  test('GET /api/v1/listings/:slug should return full details and images for The Courtyard Studio', async ({ request }) => {
    const response = await request.get(`${listingApiUrl}/api/v1/listings/${targetVilla.slug}`);
    expect(response.status()).toBe(200);

    const data = await response.json();
    expect(data.listing).toBeDefined();
    expect(data.listing.id).toBe(targetVilla.id);
    expect(data.listing.name).toBe(targetVilla.name);
    expect(data.listing.slug).toBe(targetVilla.slug);
    expect(data.listing.price_per_night).toBe(targetVilla.pricePerNight);
    expect(data.listing.max_guests).toBe(4);

    // Verify images array
    expect(Array.isArray(data.images)).toBe(true);
    expect(data.images.length).toBeGreaterThan(0);
    // Listing photos live in Cloud Storage (the only image host the portals' CSP allows).
    expect(data.images[0].url).toContain('https://storage.googleapis.com/');
  });

  test.describe('server-side listing authorization (#58)', () => {
    test.beforeAll(() => {
      psql(`
        INSERT INTO "user" (id, email, password_hash, first_name, last_name, roles,
                            is_active, is_verified, default_currency, created_at, updated_at)
        VALUES ('${OTHER_HOST_ID}', 'other.host.authz@example.com', 'x', 'Other', 'Host',
                '{host}', true, true, 'USD', NOW(), NOW())
        ON CONFLICT (email) DO NOTHING;
      `);
    });

    test('PATCH /api/v1/listings/:id without a JWT is 401', async ({ request }) => {
      const response = await request.patch(`${listingApiUrl}/api/v1/listings/${targetVilla.id}`, {
        data: { name: targetVilla.name },
      });
      expect(response.status()).toBe(401);
    });

    test('PATCH /api/v1/listings/:id by another host is 403', async ({ request }) => {
      const response = await request.patch(`${listingApiUrl}/api/v1/listings/${targetVilla.id}`, {
        headers: { Authorization: `Bearer ${signTestJwt(OTHER_HOST_ID)}` },
        data: { name: targetVilla.name },
      });
      expect(response.status()).toBe(403);
    });

    test('POST /api/v1/listings/:id/price-overrides without a JWT is 401', async ({ request }) => {
      const response = await request.post(
        `${listingApiUrl}/api/v1/listings/${targetVilla.id}/price-overrides`,
        { data: { start_date: '2031-01-01', end_date: '2031-01-05', nightly_rate: '100.00', min_nights: 1 } },
      );
      expect(response.status()).toBe(401);
    });
  });
});

