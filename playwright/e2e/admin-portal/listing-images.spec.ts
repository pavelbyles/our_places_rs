import { test, expect } from '../fixtures/auth.fixture.js';
import { createHost, loginAdminAs } from '../fixtures/admin-data.js';
import { psql, SEEDED_LISTING_ID } from '../fixtures/bookings-data.js';

// Inventory A15 (owner decision): photo upload is new in the redesigned
// console. Files go straight to Cloud Storage via signed URLs.
const NEW_ONLY = { tag: '@new-only' };

const STORAGE_HOSTS = ['storage.googleapis.com', 'localhost:4443'];

async function emulatorRunning(): Promise<boolean> {
  return fetch('http://localhost:4443/storage/v1/b')
    .then((res) => res.ok)
    .catch(() => false);
}

// A 1x1 transparent PNG.
const PNG = Buffer.from(
  'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==',
  'base64',
);

async function newListing(page: import('@playwright/test').Page): Promise<string> {
  await page.goto('/admin/listings/new');
  await page.locator('#listing-name').fill(`E2E listing ${Date.now()}`);
  await page.locator('input[name="price_per_night"]').fill('200');
  await page.locator('#btn-save-listing').click();
  await expect(page).toHaveURL(/\/admin\/listings\/[0-9a-f-]{36}$/);
  return page.url().split('/').pop() ?? '';
}

test.describe('Admin Portal - Listing Photos', () => {
  test('uploads a photo straight to Cloud Storage', NEW_ONLY, async ({ authenticatedAdminPage: page }) => {
    const id = await newListing(page);
    const gcsPuts: string[] = [];
    // Cloud Storage, or the local emulator when devenv's STORAGE_EMULATOR_HOST is set.
    await page.route((url) => STORAGE_HOSTS.includes(url.host), async (route) => {
      gcsPuts.push(`${route.request().method()} ${route.request().postDataBuffer()?.length ?? 0}`);
      await route.fulfill({ status: 200, body: '' });
    });
    const portalBodies: number[] = [];
    page.on('request', (req) => {
      if (new URL(req.url()).host === 'localhost:3012' && req.method() !== 'GET') {
        portalBodies.push(req.postDataBuffer()?.length ?? 0);
      }
    });

    await page.goto(`/admin/listings/${id}/edit`);
    await page.locator('#image-upload-input').setInputFiles({ name: 'pool.png', mimeType: 'image/png', buffer: PNG });
    await page.locator('#btn-upload-images').click();

    await expect(page.locator('[data-upload-status]')).toContainText('Uploaded 1 photo');
    expect(gcsPuts).toEqual([`PUT ${PNG.length}`]);
    // The portal only received the small JSON request, never the image.
    expect(portalBodies.every((n) => n < 200)).toBe(true);
    expect(psql(`SELECT count(*) FROM listing_image WHERE listing_id = '${id}';`)).toBe('1');
  });

  test('shows an uploaded photo once, after processing, without a reload', NEW_ONLY, async ({ authenticatedAdminPage: page }) => {
    test.skip(!(await emulatorRunning()), 'needs the local Cloud Storage emulator (gcs-start) and image_worker');
    test.setTimeout(120_000);
    const id = await newListing(page);
    await page.goto(`/admin/listings/${id}/edit`);
    await expect(page.locator('#listing-photos li')).toHaveCount(0);
    await page.locator('#image-upload-input').setInputFiles({ name: 'pool.png', mimeType: 'image/png', buffer: PNG });
    await page.locator('#btn-upload-images').click();

    await expect(page.locator('[data-upload-status]')).toContainText('Photos ready', { timeout: 100_000 });
    // One tile per photo, not one per resized variant.
    await expect(page.locator('#listing-photos li')).toHaveCount(1);
    await expect(page.locator('#listing-photos img')).toHaveAttribute('src', /optimized\/desktop\//);
  });

  test('rejects unsupported files before asking for upload URLs', NEW_ONLY, async ({ authenticatedAdminPage: page }) => {
    const id = await newListing(page);
    await page.goto(`/admin/listings/${id}/edit`);
    await page.locator('#image-upload-input').setInputFiles({ name: 'notes.txt', mimeType: 'text/plain', buffer: Buffer.from('hello') });
    await page.locator('#btn-upload-images').click();
    await expect(page.locator('[data-upload-status]')).toContainText('Upload failed');
    expect(psql(`SELECT count(*) FROM listing_image WHERE listing_id = '${id}';`)).toBe('0');
  });

  test('removes a photo and its resized variants', NEW_ONLY, async ({ authenticatedAdminPage: page }) => {
    const id = await newListing(page);
    psql(
      `INSERT INTO listing_image (listing_id, client_file_id, status, upload_url, is_primary)
       VALUES ('${id}', '0', 'Processed', 'https://storage.googleapis.com/our-places-e2e/listings/remove-me', TRUE);`,
    );
    await page.goto(`/admin/listings/${id}/edit`);
    await expect(page.locator('#listing-photos li')).toHaveCount(1);

    page.once('dialog', (dialog) => dialog.accept());
    await page.getByRole('button', { name: 'Remove photo 1' }).click();

    await expect(page.locator('#listing-photos li')).toHaveCount(0);
    await expect(page.locator('#listing-photos-slot')).toContainText('No processed photos yet.');
    expect(psql(`SELECT count(*) FROM listing_image WHERE listing_id = '${id}';`)).toBe('0');
  });

  test("keeps hosts from removing another host's photos", NEW_ONLY, async ({ browser }) => {
    const context = await browser.newContext({ baseURL: 'http://localhost:3012' });
    const page = await context.newPage();
    await loginAdminAs(page, createHost());
    const before = psql(`SELECT count(*) FROM listing_image WHERE listing_id = '${SEEDED_LISTING_ID}';`);
    const res = await page.request.post(`/admin/listings/${SEEDED_LISTING_ID}/images/remove`, {
      form: { image_id: '00000000-0000-0000-0000-000000000000' },
      headers: { Origin: 'http://localhost:3012' },
    });
    expect(res.status()).toBe(404);
    expect(psql(`SELECT count(*) FROM listing_image WHERE listing_id = '${SEEDED_LISTING_ID}';`)).toBe(before);
    await context.close();
  });

  test("keeps hosts from uploading to another host's listing", NEW_ONLY, async ({ browser }) => {
    const context = await browser.newContext({ baseURL: 'http://localhost:3012' });
    const page = await context.newPage();
    await loginAdminAs(page, createHost());
    const res = await page.request.post(`/admin/listings/${SEEDED_LISTING_ID}/images/presign`, {
      data: { files: [{ content_type: 'image/png', size_bytes: 10 }] },
      headers: { Origin: 'http://localhost:3012' },
    });
    expect(res.status()).toBe(404);
    await context.close();
  });
});
