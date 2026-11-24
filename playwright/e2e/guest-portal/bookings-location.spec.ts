import { test, expect } from '../fixtures/base.fixture.js';
import { KNOWN_LOCATION } from '../fixtures/test-data.js';
import { createBooking, createGuest, loginGuest } from '../fixtures/bookings-data.js';
import { SWIFTSHADER, watchMapRequests } from '../fixtures/map-watch.js';

/** Spec 103 §1.8: the exact location on a guest's confirmed booking. */

test.use(SWIFTSHADER);

test.describe('Guest Portal - booking location', () => {
  test('a confirmed booking shows the exact pin; a pending one shows none', async ({ page, request }) => {
    const watch = watchMapRequests(page);
    const guest = createGuest();
    const confirmed = await createBooking(request, guest.id, { confirmed: true });
    const pending = await createBooking(request, guest.id);
    await loginGuest(page, guest);
    await page.goto('/bookings');

    const block = page.locator(`#booking-location-${confirmed.id}`);
    const map = block.locator('[data-map="booking"]');
    await expect(map).toHaveAttribute('data-precision', 'exact');
    await expect(map).toHaveAttribute('data-lat', String(KNOWN_LOCATION.latitude));
    await expect(map).toHaveAttribute('data-lng', String(KNOWN_LOCATION.longitude));
    await expect(block).toContainText('Exact location for your stay.');
    await map.scrollIntoViewIfNeeded();
    await expect(map).toHaveAttribute('data-map-state', 'ready', { timeout: 30000 });
    await expect(map.locator('.maplibregl-marker')).toHaveCount(1);

    await expect(page.locator(`#booking-${pending.id}`)).toBeVisible();
    await expect(page.locator(`#booking-location-${pending.id}`)).toHaveCount(0);
    await expect(page.locator(`#booking-${pending.id} [data-map]`)).toHaveCount(0);
    expect(watch.failed).toEqual([]);
  });
});
