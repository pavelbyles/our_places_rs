import { test, expect } from '@playwright/test';
import { KNOWN_LISTINGS } from '../fixtures/test-data.js';

test.describe('Backend API Validation - Listing Service', () => {
  const listingApiUrl = process.env.LISTING_API_URL || 'http://127.0.0.1:8082';
  const targetVilla = KNOWN_LISTINGS.courtyardStudio;

  test('GET /api/v1/listings should return active listings with valid schema', async ({ request }) => {
    const response = await request.get(`${listingApiUrl}/api/v1/listings`);
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
    expect(data.images[0].url).toContain('https://images.unsplash.com/');
  });
});
