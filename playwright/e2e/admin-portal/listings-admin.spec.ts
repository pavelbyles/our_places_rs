import { test, expect } from '../fixtures/auth.fixture.js';
import { createHost, loginAdminAs } from '../fixtures/admin-data.js';
import { psql, SEEDED_LISTING_ID } from '../fixtures/bookings-data.js';
import { KNOWN_LISTINGS, TEST_CONFIG } from '../fixtures/test-data.js';

test.describe('Admin Portal - Listings Management', () => {
  test.beforeEach(async ({ authenticatedAdminPage }) => {
    await authenticatedAdminPage.goto('/admin/listings');
  });

  test('should render listings inventory page and title', async ({ authenticatedAdminPage }) => {
    const heading = authenticatedAdminPage.locator('h1');
    await expect(heading).toContainText('Listings Management');

    // Check action buttons
    const createBtn = authenticatedAdminPage.locator('main a[href*="/listings/new"]').first();
    await expect(createBtn).toBeVisible();
  });

  test('should provide search and filter controls', async ({ authenticatedAdminPage }) => {
    const searchInput = authenticatedAdminPage.locator('input[placeholder*="Search" i], input[type="search"]').first();
    if (await searchInput.isVisible()) {
      await searchInput.fill('Kingston');
      await expect(searchInput).toHaveValue('Kingston');
    }
  });

  // The legacy create/clone pages never run (their inline script fails to
  // parse, inventory A11/A12/A14), so these are new-portal only.
  test('creates a listing from the studio form', { tag: '@new-only' }, async ({ authenticatedAdminPage: page }) => {
    const name = `E2E listing ${Date.now()}`;
    await page.goto('/admin/listings/new');
    await page.locator('#listing-name').fill(name);
    await page.locator('input[name="city"]').fill('Ocho Rios');
    await page.locator('input[name="price_per_night"]').fill('825.50');
    await page.locator('input[name="cleaning_fee"]').fill('90');
    await page.locator('input[name="max_guests"]').fill('6');
    await page.locator('input[name="guests_included"]').fill('4');
    await page.locator('#btn-save-listing').click();

    await expect(page).toHaveURL(/\/admin\/listings\/[0-9a-f-]{36}$/);
    await expect(page.locator('h1')).toHaveText(name);
    expect(psql(`SELECT price_per_night, cleaning_fee, max_guests FROM listing WHERE name = '${name}';`)).toBe('825.50|90.00|6');
  });

  test('shows validation errors without saving', { tag: '@new-only' }, async ({ authenticatedAdminPage: page }) => {
    const name = `E2E listing ${Date.now()}`;
    await page.goto('/admin/listings/new');
    await page.locator('#listing-name').fill(name);
    await page.locator('input[name="price_per_night"]').fill('100');
    await page.locator('input[name="max_guests"]').fill('2');
    await page.locator('input[name="guests_included"]').fill('5');
    await page.locator('#btn-save-listing').click();
    await expect(page.locator('#listing-form-errors')).toContainText('Guests included cannot exceed');
    await expect(page.locator('#listing-name')).toHaveValue(name);
    expect(psql(`SELECT count(*) FROM listing WHERE name = '${name}';`)).toBe('0');
  });

  test('pre-fills the studio when cloning', { tag: '@new-only' }, async ({ authenticatedAdminPage: page }) => {
    await page.goto(`/admin/listings/clone/${KNOWN_LISTINGS.courtyardStudio.slug}`);
    await expect(page.locator('#listing-name')).toHaveValue(`Copy of ${KNOWN_LISTINGS.courtyardStudio.name}`);
    await expect(page.locator('input[name="price_per_night"]')).toHaveValue('650');
  });

  test("keeps hosts out of another host's listing", { tag: '@new-only' }, async ({ browser }) => {
    const context = await browser.newContext({ baseURL: 'http://localhost:3012' });
    const page = await context.newPage();
    await loginAdminAs(page, createHost());
    for (const path of [`/admin/listings/${SEEDED_LISTING_ID}`, `/admin/listings/clone/${KNOWN_LISTINGS.courtyardStudio.slug}`]) {
      const res = await page.goto(path);
      expect(res?.status(), path).toBe(404);
    }
    await page.goto('/admin/listings');
    await expect(page.locator('#admin-listings-table')).not.toContainText(KNOWN_LISTINGS.courtyardStudio.name);
    await context.close();
  });

  test('marks required fields and counts description characters', { tag: '@new-only' }, async ({ authenticatedAdminPage: page }) => {
    await page.goto('/admin/listings/new');
    await expect(page.locator('#listing-required-note')).toHaveText('Required');
    // Every required control's label carries exactly one dot.
    const requiredCount = await page.locator('#listing-form [required]').count();
    expect(requiredCount).toBeGreaterThan(5);
    await expect(page.locator('#listing-form [data-required-dot]')).toHaveCount(requiredCount);
    await expect(page.locator('label:has([name="name"]) [data-required-dot]')).toHaveCount(1);
    await expect(page.locator('label:has([name="city"]) [data-required-dot]')).toHaveCount(0);
    for (const name of ['name', 'country', 'price_per_night', 'max_guests', 'minimum_stay']) {
      await expect(page.locator(`[name="${name}"]`)).toHaveAttribute('required', '');
    }
    await expect(page.locator('[name="cleaning_fee"]')).not.toHaveAttribute('required', '');
    await expect(page.locator('#listing-description')).toHaveAttribute('maxlength', '2000');
    await expect(page.locator('#listing-description-count')).toHaveText('0 / 2000');
    await page.locator('#listing-description').fill('A quiet villa by the sea.');
    await expect(page.locator('#listing-description-count')).toHaveText('25 / 2000');
  });

  test('edits a listing and keeps its owner', { tag: '@new-only' }, async ({ authenticatedAdminPage: page }) => {
    const name = `E2E listing ${Date.now()}`;
    await page.goto('/admin/listings/new');
    await page.locator('#listing-name').fill(name);
    await page.locator('input[name="price_per_night"]').fill('300');
    await page.locator('#btn-save-listing').click();
    await expect(page).toHaveURL(/\/admin\/listings\/[0-9a-f-]{36}$/);
    const id = page.url().split('/').pop() ?? '';
    const owner = psql(`SELECT user_id FROM listing WHERE id = '${id}';`);

    await page.goto(`/admin/listings/${id}/edit`);
    await expect(page.locator('input[name="price_per_night"]')).toHaveValue('300');
    await page.locator('input[name="price_per_night"]').fill('345.75');
    await page.locator('input[name="cleaning_fee"]').fill('60');
    await page.locator('#btn-save-listing').click();
    await expect(page).toHaveURL(new RegExp(`/admin/listings/${id}$`));
    expect(psql(`SELECT price_per_night, cleaning_fee, user_id FROM listing WHERE id = '${id}';`)).toBe(`345.75|60.00|${owner}`);
  });

  test('adds amenities that guests see, keeping the other listing details', { tag: '@new-only' }, async ({ authenticatedAdminPage: page }) => {
    const name = `E2E amenities ${Date.now()}`;
    await page.goto('/admin/listings/new');
    await page.locator('#listing-name').fill(name);
    await page.locator('input[name="price_per_night"]').fill('300');
    await page.locator('#listing-amenities').fill('Private pool\nWi-Fi\n\nwi-fi');
    await page.locator('#btn-save-listing').click();
    await expect(page).toHaveURL(/\/admin\/listings\/[0-9a-f-]{36}$/);
    const id = page.url().split('/').pop() ?? '';
    await expect(page.locator('#listing-amenities-list li')).toHaveText(['Private pool', 'Wi-Fi']);

    // Details set elsewhere (pre-arrival email data) must survive an edit.
    psql(`UPDATE listing SET listing_details = listing_details || '{"wifi_ssid": "E2E-Net"}'::jsonb WHERE id = '${id}';`);
    await page.goto(`/admin/listings/${id}/edit`);
    await expect(page.locator('#listing-amenities')).toHaveValue('Private pool\nWi-Fi');
    await page.locator('#listing-amenities').fill('Private pool\nOcean view');
    await page.locator('#btn-save-listing').click();
    await expect(page).toHaveURL(new RegExp(`/admin/listings/${id}$`));
    await expect(page.locator('#listing-amenities-list li')).toHaveText(['Private pool', 'Ocean view']);
    expect(psql(`SELECT listing_details->>'wifi_ssid' FROM listing WHERE id = '${id}';`)).toBe('E2E-Net');

    const slug = psql(`SELECT slug FROM listing WHERE id = '${id}';`);
    await page.goto(`${TEST_CONFIG.guestPortalUrl}/listings/${slug}`);
    await expect(page.getByText('Ocean view')).toBeVisible();
    await expect(page.getByText('Private pool')).toBeVisible();
  });
});

