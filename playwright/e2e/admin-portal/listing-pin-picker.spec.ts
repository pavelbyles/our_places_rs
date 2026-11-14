import { test, expect } from '../fixtures/auth.fixture.js';
import { createListingCopy, psql } from '../fixtures/bookings-data.js';
import { KNOWN_LOCATION } from '../fixtures/test-data.js';
import { SWIFTSHADER, watchMapRequests } from '../fixtures/map-watch.js';

/**
 * Spec 103 §1.9: the admin pin picker. The edit page starts from the stored
 * point (the manager endpoint), a drag rewrites both inputs, and the saved
 * values and privacy setting come back after a reload.
 */

test.use(SWIFTSHADER);

const SIX_DP = /^-?\d+\.\d{6}$/;

test.describe('Admin Portal - pin picker', () => {
  test('drag the pin, save, and the stored point and privacy come back', async ({ authenticatedAdminPage: page }) => {
    const watch = watchMapRequests(page);
    const listing = createListingCopy();
    await page.goto(`/admin/listings/${listing.id}/edit`);

    const lat = page.locator('input[name="latitude"]');
    const lng = page.locator('input[name="longitude"]');
    // The stored point, never the public one.
    await expect(lat).toHaveValue(String(KNOWN_LOCATION.latitude));
    await expect(lng).toHaveValue(String(KNOWN_LOCATION.longitude));

    const map = page.locator('[data-map="picker"]');
    await map.scrollIntoViewIfNeeded();
    await expect(map).toHaveAttribute('data-map-state', 'ready', { timeout: 30000 });
    const pin = map.locator('.maplibregl-marker');
    await expect(pin).toHaveCount(1);
    const box = await pin.boundingBox();
    if (!box) throw new Error('no pin');
    const x = box.x + box.width / 2;
    const y = box.y + box.height / 2;
    await page.mouse.move(x, y);
    await page.mouse.down();
    await page.mouse.move(x + 40, y + 20, { steps: 8 });
    await page.mouse.move(x + 80, y + 40, { steps: 8 });
    await page.mouse.up();
    await expect(lat).not.toHaveValue(String(KNOWN_LOCATION.latitude));
    const newLat = await lat.inputValue();
    const newLng = await lng.inputValue();
    expect(newLat).toMatch(SIX_DP);
    expect(newLng).toMatch(SIX_DP);

    await page.locator('input[name="location_privacy"][value="exact"]').check();
    await page.locator('#btn-save-listing').click();
    await expect(page).toHaveURL(new RegExp(`/admin/listings/${listing.id}$`));
    expect(psql(`SELECT latitude, longitude, location_privacy FROM listing WHERE id = '${listing.id}';`)).toBe(
      `${Number(newLat)}|${Number(newLng)}|exact`,
    );

    await page.goto(`/admin/listings/${listing.id}/edit`);
    await expect(lat).toHaveValue(String(Number(newLat)));
    await expect(lng).toHaveValue(String(Number(newLng)));
    await expect(page.locator('input[name="location_privacy"][value="exact"]')).toBeChecked();
    expect(watch.failed).toEqual([]);
  });

  test('a new listing starts with no pin', async ({ authenticatedAdminPage: page }) => {
    await page.goto('/admin/listings/new');
    const map = page.locator('[data-map="picker"]');
    await map.scrollIntoViewIfNeeded();
    await expect(map).toHaveAttribute('data-map-state', 'ready', { timeout: 30000 });
    await expect(map.locator('.maplibregl-marker')).toHaveCount(0);
    await expect(page.locator('input[name="location_privacy"][value="approximate"]')).toBeChecked();
  });
});
