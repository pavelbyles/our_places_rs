import { test, expect } from '../fixtures/auth.fixture.js';
import { psql, SEEDED_LISTING_ID } from '../fixtures/bookings-data.js';

// Parity inventory A17/A18: the legacy add/remove handlers are stubs that
// render hardcoded rows and never reach listing_api. These run on the new portal.
const NEW_ONLY = { tag: '@new-only' };

const overrides = (start: string) =>
  psql(`SELECT count(*) FROM listing_price_overrides
        WHERE listing_id = '${SEEDED_LISTING_ID}' AND start_date = '${start}';`);

/** A random 3-night window in 2037, unique enough for parallel runs. */
function randomWindow(): { start: string; end: string } {
  const start = new Date(Date.UTC(2037, 0, 1 + Math.floor(Math.random() * 350)));
  const end = new Date(start.getTime() + 3 * 86_400_000);
  return { start: start.toISOString().slice(0, 10), end: end.toISOString().slice(0, 10) };
}

test.describe('Admin Portal - Seasonal Pricing Overrides', () => {
  test('renders the override form and table', async ({ authenticatedAdminPage: page }) => {
    const resp = await page.goto(`/admin/listings/${SEEDED_LISTING_ID}/pricing`);
    expect(resp?.status()).toBe(200);
    await expect(page.locator('form input[name="start_date"]')).toBeVisible();
    await expect(page.locator('form input[name="end_date"]')).toBeVisible();
    await expect(page.locator('#price-overrides-container')).toBeVisible();
  });

  test('legacy unprefixed path still reaches the page', async ({ authenticatedAdminPage: page }) => {
    await page.goto(`/listings/${SEEDED_LISTING_ID}/pricing`);
    await expect(page.locator('#price-overrides-container')).toBeVisible();
  });

  test('adds and removes a seasonal override', NEW_ONLY, async ({ authenticatedAdminPage: page }) => {
    const { start, end } = randomWindow();
    await page.goto(`/admin/listings/${SEEDED_LISTING_ID}/pricing`);

    await page.locator('form input[name="start_date"]').fill(start);
    await page.locator('form input[name="end_date"]').fill(end);
    await page.locator('form input[name="price"]').fill('1234');
    await page.locator('form input[name="min_nights"]').fill('2');
    await page.locator('form:has(input[name="start_date"]) [type="submit"]').click();

    await expect.poll(() => overrides(start), { timeout: 10_000 }).toBe('1');
    const row = page.locator('#price-overrides-container tr', { hasText: '1,234' });
    await expect(row).toBeVisible();

    await row.getByRole('button', { name: /remove/i }).click();
    await expect.poll(() => overrides(start), { timeout: 10_000 }).toBe('0');
  });
});
