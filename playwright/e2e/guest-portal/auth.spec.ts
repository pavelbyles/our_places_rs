import { test, expect } from '@playwright/test';
import { TEST_USERS } from '../fixtures/test-data.js';

test.describe('Guest Portal - Authentication Flows', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/login');
  });

  test('should render guest login card with welcome header', async ({ page }) => {
    const heading = page.locator('h2');
    await expect(heading).toContainText('Welcome Back');
    await expect(page.locator('#guest-email')).toBeVisible();
    await expect(page.locator('#guest-password')).toBeVisible();
    await expect(page.locator('#btn-guest-login')).toBeVisible();
  });

  test('should enforce input validation for empty or malformed email', async ({ page }) => {
    const emailInput = page.locator('#guest-email');
    await emailInput.fill('invalid-email-format');
    await page.locator('#btn-guest-login').click();

    const isInvalid = await emailInput.evaluate((el: HTMLInputElement) => !el.checkValidity());
    expect(isInvalid).toBe(true);
  });

  test('should display error message on invalid guest credentials', async ({ page }) => {
    await page.locator('#guest-email').fill('nonexistent@example.com');
    await page.locator('#guest-password').fill('wrongpassword123');
    await page.locator('#btn-guest-login').click();

    const errorAlert = page.locator('#guest-login-error');
    await expect(errorAlert).toBeVisible({ timeout: 5000 });
    await expect(page.locator('#guest-login-error-text')).toContainText(/Invalid|Authentication error|failed/i);
  });

  test('should provide links to registration and passwordless code login', async ({ page }) => {
    // Scope to the page body to avoid strict mode violations with navbar and footer links
    const registerLink = page.getByRole('main').locator('a[href="/register"]');
    await expect(registerLink).toBeVisible();

    const verifyLink = page.getByRole('main').locator('a[href="/verify"]');
    await expect(verifyLink).toBeVisible();
  });
});
