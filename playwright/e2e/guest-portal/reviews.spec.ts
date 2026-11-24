import { test, expect } from '../fixtures/base.fixture.js';
import { createBooking, createGuest, reviewTokenForPastStay } from '../fixtures/bookings-data.js';

// Parity inventory G33: the legacy review form posts to a static success
// page and never reaches listing_api. These run on the new portal.
const NEW_ONLY = { tag: '@new-only' };

test.describe('Guest Portal - Verified Reviews', () => {
  test('renders the review confirmation page', async ({ page }) => {
    const resp = await page.goto('/reviews/success');
    expect(resp?.status()).toBe(200);
    await expect(page.locator('h1')).toBeVisible();
    await expect(page.getByRole('main').locator('a[href="/listings"]')).toBeVisible();
  });

  test('submits a verified review with a review token', NEW_ONLY, async ({ page, request }) => {
    const guest = createGuest();
    const booking = await createBooking(request, guest.id, { confirmed: true });
    const token = await reviewTokenForPastStay(request, guest, booking);

    const resp = await page.goto(`/reviews/submit?token=${encodeURIComponent(token)}`);
    expect(resp?.status()).toBe(200);
    for (const category of ['cleanliness', 'accuracy', 'location', 'value']) {
      // People click the visible star label; the radio itself is visually hidden.
      await page.locator(`label:has(input[name="${category}_rating"][value="5"])`).click();
      await expect(page.locator(`input[name="${category}_rating"][value="5"]`)).toBeChecked();
    }
    await page.locator('textarea[name="public_review_text"]').fill('Wonderful stay, spotless villa.');
    await page.getByRole('button', { name: /submit/i }).click();
    await expect(page).toHaveURL(/\/reviews\/success$/);
  });

  test('rejects an unknown review token', NEW_ONLY, async ({ page }) => {
    const resp = await page.goto('/reviews/submit?token=not-a-real-token');
    expect(resp?.status()).toBe(404);
  });
});
