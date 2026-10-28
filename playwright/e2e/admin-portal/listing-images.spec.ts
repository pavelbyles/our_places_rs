import { test, expect } from '../fixtures/auth.fixture.js';
import { createHost, loginAdminAs } from '../fixtures/admin-data.js';
import { psql, SEEDED_LISTING_ID } from '../fixtures/bookings-data.js';

// Inventory A15 (owner decision): photo upload is new in the redesigned
// console. Files go straight to Cloud Storage via signed URLs.
const NEW_ONLY = { tag: '@new-only' };

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
    await page.route('https://storage.googleapis.com/**', async (route) => {
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

  test('rejects unsupported files before asking for upload URLs', NEW_ONLY, async ({ authenticatedAdminPage: page }) => {
    const id = await newListing(page);
    await page.goto(`/admin/listings/${id}/edit`);
    await page.locator('#image-upload-input').setInputFiles({ name: 'notes.txt', mimeType: 'text/plain', buffer: Buffer.from('hello') });
    await page.locator('#btn-upload-images').click();
    await expect(page.locator('[data-upload-status]')).toContainText('Upload failed');
    expect(psql(`SELECT count(*) FROM listing_image WHERE listing_id = '${id}';`)).toBe('0');
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
