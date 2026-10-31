import { test, expect } from '../fixtures/base.fixture.js';
import { createHost, loginAdminAs } from '../fixtures/admin-data.js';
import { psql } from '../fixtures/bookings-data.js';
import { TEST_USERS } from '../fixtures/test-data.js';

test.describe('Admin Portal - User Administration', () => {
  test('lists and searches user accounts', async ({ page }) => {
    // A fresh account: the legacy directory only loads the newest 100 users
    // before filtering, and every test run adds users.
    const host = createHost();
    await loginAdminAs(page, TEST_USERS.admin);
    await page.goto(`/admin/users?q=${encodeURIComponent(host.email)}`);
    await expect(page.locator('#user-search-input')).toBeVisible();
    await expect(page.getByRole('main')).toContainText(host.email);
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

  test('changes a user\'s profile URL', { tag: '@new-only' }, async ({ page }) => {
    const host = createHost();
    const current = psql(`SELECT slug FROM "user" WHERE id = '${host.id}';`);
    await loginAdminAs(page, TEST_USERS.admin);
    await page.goto(`/admin/users?q=${encodeURIComponent(host.email)}`);

    await page.getByRole('row', { name: new RegExp(host.email) }).getByRole('button', { name: /edit/i }).click();
    await expect(page.locator('#edit-user-slug')).toHaveValue(current);
    const wanted = `admin-set-${Date.now()}`;
    await page.locator('#edit-user-slug').fill(wanted);
    await page.locator('#edit-user-save-btn').click();

    await expect.poll(() => psql(`SELECT slug FROM "user" WHERE id = '${host.id}';`), { timeout: 10_000 }).toBe(wanted);
  });

  test('keeps hosts out of user administration', async ({ page }) => {
    await loginAdminAs(page, createHost());
    await page.goto('/admin/users/new');
    await expect(page.locator('#admin-create-user-form')).toHaveCount(0);
    await page.goto('/admin/users');
    await expect(page.locator('#user-search-input')).toHaveCount(0);
  });

  test('rejects user changes posted by a host', { tag: '@new-only' }, async ({ page }) => {
    const target = createHost();
    await loginAdminAs(page, createHost());
    await page.goto('/admin');
    const origin = new URL(page.url()).origin;
    for (const path of ['/admin/users', `/admin/users/${target.id}`, `/admin/users/${target.id}/delete`, `/admin/users/${target.id}/restore`]) {
      const resp = await page.request.post(path, { form: { first_name: 'X', purge: 'true' }, headers: { Origin: origin } });
      expect(resp.status(), path).toBe(403);
    }
    expect(psql(`SELECT deleted_at IS NULL FROM "user" WHERE id = '${target.id}';`)).toBe('t');
  });

  test('archives and restores an account', { tag: '@new-only' }, async ({ page }) => {
    const host = createHost();
    await loginAdminAs(page, TEST_USERS.admin);
    await page.goto(`/admin/users?q=${encodeURIComponent(host.email)}`);

    await page.getByRole('row', { name: new RegExp(host.email) }).getByRole('button', { name: 'Archive' }).click();
    const dialog = page.getByRole('alertdialog', { name: 'Archive this account?' });
    await dialog.getByRole('button', { name: 'Archive account' }).click();
    await expect(page.locator('#users-notice')).toContainText('archived');
    expect(psql(`SELECT deleted_at IS NOT NULL FROM "user" WHERE id = '${host.id}';`)).toBe('t');

    await page.goto(`/admin/users?status=archived&q=${encodeURIComponent(host.email)}`);
    await page.getByRole('row', { name: new RegExp(host.email) }).getByRole('button', { name: 'Restore' }).click();
    await expect(page.locator('#users-notice')).toContainText('restored');
    expect(psql(`SELECT deleted_at IS NULL FROM "user" WHERE id = '${host.id}';`)).toBe('t');
  });

  test('refuses to delete the signed-in administrator', { tag: '@new-only' }, async ({ page }) => {
    await loginAdminAs(page, TEST_USERS.admin);
    const adminId = psql(`SELECT id FROM "user" WHERE email = '${TEST_USERS.admin.email}';`);
    await page.goto('/admin/users');
    const resp = await page.request.post(`/admin/users/${adminId}/delete`, {
      form: { purge: 'true' },
      headers: { Origin: new URL(page.url()).origin },
      maxRedirects: 0,
    });
    expect(resp.status()).toBe(303);
    expect(resp.headers()['location']).toContain('notice=self');
    expect(psql(`SELECT deleted_at IS NULL FROM "user" WHERE id = '${adminId}';`)).toBe('t');
  });
});
