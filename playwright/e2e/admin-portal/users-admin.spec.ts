import { test, expect } from '../fixtures/base.fixture.js';
import { createHost, loginAdminAs } from '../fixtures/admin-data.js';
import { psql } from '../fixtures/bookings-data.js';
import { TEST_USERS } from '../fixtures/test-data.js';

test.describe('Admin Portal - User Administration', () => {
  test('lists and searches user accounts', async ({ page }) => {
    await loginAdminAs(page, TEST_USERS.admin);
    await page.goto(`/admin/users?q=${encodeURIComponent(TEST_USERS.guest.email)}`);
    await expect(page.locator('#user-search-input')).toBeVisible();
    await expect(page.getByRole('main')).toContainText(TEST_USERS.guest.email);
  });

  test('creates a user account', async ({ page }) => {
    const email = `e2e.created.${Date.now()}.${Math.floor(Math.random() * 1e6)}@example.com`;
    await loginAdminAs(page, TEST_USERS.admin);
    await page.goto('/admin/users/new');

    await page.locator('#new-user-first-name').fill('Avery');
    await page.locator('#new-user-last-name').fill('Created');
    await page.locator('#new-user-email').fill(email);
    await page.locator('#new-user-password').fill('createdPassword123!');
    await page.locator('#role-booker').check();
    await page.locator('#btn-create-user').click();

    await expect(page).toHaveURL(/\/admin\/users$/, { timeout: 15_000 });
    expect(psql(`SELECT first_name FROM "user" WHERE email = '${email}';`)).toBe('Avery');
  });

  test('edits a user name', async ({ page }) => {
    const host = createHost();
    await loginAdminAs(page, TEST_USERS.admin);
    await page.goto(`/admin/users?q=${encodeURIComponent(host.email)}`);

    await page.getByRole('row', { name: new RegExp(host.email) }).getByRole('button', { name: /edit/i }).click();
    await page.locator('#edit-user-first-name').fill('Quinn');
    await page.locator('#edit-user-save-btn').click();

    await expect.poll(() => psql(`SELECT first_name FROM "user" WHERE id = '${host.id}';`), { timeout: 10_000 }).toBe('Quinn');
  });

  test('keeps hosts out of user administration', async ({ page }) => {
    await loginAdminAs(page, createHost());
    await page.goto('/admin/users/new');
    await expect(page.locator('#admin-create-user-form')).toHaveCount(0);
    await page.goto('/admin/users');
    await expect(page.locator('#user-search-input')).toHaveCount(0);
  });
});
