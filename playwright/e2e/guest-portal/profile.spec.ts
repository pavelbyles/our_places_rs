import { test, expect } from '../fixtures/base.fixture.js';
import { createGuest, loginGuest, psql } from '../fixtures/bookings-data.js';

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

  test('defaults the profile URL to first-last and lets the guest change it', NEW_ONLY, async ({ page }) => {
    const guest = createGuest();
    await loginGuest(page, guest);
    await page.goto('/profile');
    const slug = page.getByLabel('Profile URL');
    await expect(slug).toHaveValue(/^casey-e2e(-\d+)?$/);

    const wanted = `casey-villas-${Date.now()}`;
    await slug.fill(`  Casey Villas ${wanted.slice('casey-villas-'.length)}! `);
    await page.getByRole('button', { name: /save/i }).click();
    await expect(page.getByRole('status')).toContainText('saved');
    await expect(slug).toHaveValue(wanted);
    expect(psql(`SELECT slug FROM "user" WHERE email = '${guest.email}';`)).toBe(wanted);
  });

  test('refuses a profile URL someone else has', NEW_ONLY, async ({ page }) => {
    const other = createGuest();
    const taken = psql(`SELECT slug FROM "user" WHERE email = '${other.email}';`);
    await loginGuest(page, createGuest());
    await page.goto('/profile');
    await page.getByLabel('Profile URL').fill(taken);
    await page.getByRole('button', { name: /save/i }).click();
    await expect(page.getByRole('alert')).toContainText(`“${taken}” is already taken`);
  });

  test('redirects a signed-out visitor to login', NEW_ONLY, async ({ page }) => {
    await page.goto('/profile');
    await expect(page).toHaveURL(/\/login/);
  });
});
