import { test as base, type Page } from '@playwright/test';
import { TEST_USERS } from './test-data.js';

export async function loginAsAdmin(page: Page, email = TEST_USERS.admin.email, password = TEST_USERS.admin.password): Promise<void> {
  await page.goto('/login');
  await page.locator('#admin-login-email').fill(email);
  await page.locator('#admin-login-password').fill(password);
  await page.locator('#btn-admin-login').click();
  // Wait for redirect to dashboard or admin area
  await page.waitForURL((url) => !url.pathname.includes('/login'), { timeout: 10000 });
}

export async function setAdminSessionStorage(page: Page, user = TEST_USERS.admin): Promise<void> {
  await page.addInitScript((u) => {
    localStorage.setItem(
      'op_auth_user',
      JSON.stringify({
        id: '01a0bcbd-014d-7062-99b2-6a42d05b9ed7',
        name: `${u.firstName} ${u.lastName}`,
        email: u.email,
        role: u.role,
      })
    );
  }, user);
}

export const test = base.extend<{
  authenticatedAdminPage: Page;
}>({
  authenticatedAdminPage: async ({ page }, use) => {
    await loginAsAdmin(page);
    await use(page);
  },
});

export { expect } from '@playwright/test';
