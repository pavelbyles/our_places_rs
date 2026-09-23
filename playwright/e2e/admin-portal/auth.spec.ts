import { test, expect } from '@playwright/test';
import { TEST_USERS } from '../fixtures/test-data.js';

test.describe('Admin Portal - Authentication & Route Guard', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/login');
  });

  test('should display executive admin login card', async ({ page }) => {
    await expect(page).toHaveTitle(/Admin/i);
    await expect(page.locator('h2')).toContainText('Sign in to Our Places');
    await expect(page.locator('#admin-login-email')).toBeVisible();
    await expect(page.locator('#admin-login-password')).toBeVisible();
    await expect(page.locator('#btn-admin-login')).toBeVisible();
  });

  test('should require email and password on login form', async ({ page }) => {
    const emailInput = page.locator('#admin-login-email');
    await emailInput.fill('');
    await page.locator('#btn-admin-login').click();

    // Verify HTML5 validation or error alert
    const isInvalid = await emailInput.evaluate((el: HTMLInputElement) => !el.checkValidity());
    expect(isInvalid).toBe(true);
  });

  test('should show error notification on invalid credentials', async ({ page }) => {
    await page.locator('#admin-login-email').fill('unknown.admin@ourplaces.io');
    await page.locator('#admin-login-password').fill('incorrectPassword123');
    await page.locator('#btn-admin-login').click();

    const errorAlert = page.locator('#admin-login-error');
    await expect(errorAlert).toBeVisible({ timeout: 5000 });
    await expect(page.locator('#admin-login-error-text')).toContainText(/Invalid|Access restricted|failed/i);
  });

  test('should redirect unauthenticated users visiting protected pages to login', async ({ page }) => {
    await page.goto('/admin');
    // Guard redirects to /login with redirect parameter
    await page.waitForURL((url) => url.pathname.includes('/login'), { timeout: 8000 });
    expect(page.url()).toContain('/login');
  });
});
