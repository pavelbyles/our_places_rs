import { test, expect } from '../fixtures/base.fixture.js';
import { createGuest, loginGuest } from '../fixtures/bookings-data.js';

// Parity inventory G32: the legacy profile is static placeholder HTML
// ("Jane Doe") that only writes to localStorage. These run on the new portal.
const NEW_ONLY = { tag: '@new-only' };

test.describe('Guest Portal - Profile', () => {
  test('shows the signed-in guest details', NEW_ONLY, async ({ page }) => {
    const guest = createGuest();
    await loginGuest(page, guest);

    await page.goto('/profile');
    await expect(page.getByLabel('First name')).toHaveValue('Casey');
    await expect(page.getByRole('main')).toContainText(guest.email);
  });

  test('updates the guest name', NEW_ONLY, async ({ page }) => {
    await loginGuest(page, createGuest());
    await page.goto('/profile');

    await page.getByLabel('First name').fill('Robin');
    await page.getByRole('button', { name: /save/i }).click();
    await expect(page.getByRole('status')).toContainText('saved');
    await page.reload();
    await expect(page.getByLabel('First name')).toHaveValue('Robin');
  });

  test('redirects a signed-out visitor to login', NEW_ONLY, async ({ page }) => {
    await page.goto('/profile');
    await expect(page).toHaveURL(/\/login/);
  });
});
