import { test, expect } from '../fixtures/base.fixture.js';
import { TEST_USERS } from '../fixtures/test-data.js';
import { createGuest, createUnverifiedUser, loginGuest, psql } from '../fixtures/bookings-data.js';

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

  // Parity inventory G28: the redesigned portal keeps guest auth on the server.
  test('signs in with a server-side guest session', { tag: '@new-only' }, async ({ page, context }) => {
    const guest = createGuest();
    await loginGuest(page, guest);
    expect(
      psql(`SELECT count(*) FROM sessions WHERE user_id = '${guest.id}' AND namespace = 'guest';`),
    ).toBe('1');
    const storage = await page.evaluate(() => JSON.stringify({ ...localStorage, ...sessionStorage }));
    expect(storage).not.toMatch(/op_auth_user|token|session/i);
    const cookie = (await context.cookies()).find((c) => c.name.endsWith('op_guest_session'));
    expect(cookie?.httpOnly).toBe(true);
  });

  test('returns to a same-origin page after signing in', { tag: '@new-only' }, async ({ page }) => {
    const guest = createGuest();
    await page.goto('/login?redirect=%2Fabout');
    await page.locator('#guest-email').fill(guest.email);
    await page.locator('#guest-password').fill(guest.password);
    await page.locator('#btn-guest-login').click();
    await expect(page).toHaveURL(/\/about$/);
  });

  test('ignores an off-site redirect after signing in', { tag: '@new-only' }, async ({ page }) => {
    const guest = createGuest();
    await page.goto('/login?redirect=%2F%2Fevil.example');
    await page.locator('#guest-email').fill(guest.email);
    await page.locator('#guest-password').fill(guest.password);
    await page.locator('#btn-guest-login').click();
    await expect(page).toHaveURL(/localhost:3010\/$/);
  });

  test('signs out and removes the server session', { tag: '@new-only' }, async ({ page }) => {
    const guest = createGuest();
    await loginGuest(page, guest);
    await page.locator('#nav-user-dropdown summary').click();
    await page.getByRole('button', { name: 'Sign out' }).click();
    await expect(page).toHaveURL(/\/login$/);
    await expect(page.locator('#nav-user-dropdown')).toHaveCount(0);
    expect(psql(`SELECT count(*) FROM sessions WHERE user_id = '${guest.id}';`)).toBe('0');
  });

  test('sends an unverified guest to the code prompt, then signs them in', { tag: '@new-only' }, async ({ page }) => {
    const guest = createUnverifiedUser('booker');
    await page.locator('#guest-email').fill(guest.email);
    await page.locator('#guest-password').fill(guest.password);
    await page.locator('#btn-guest-login').click();

    await expect(page).toHaveURL(/\/verify\?email=.*&unverified=1$/);
    await expect(page.locator('#verify-email')).toHaveValue(guest.email);
    await expect(page.locator('#verify-info-text')).toContainText(/isn't verified yet/);

    await page.locator('#verify-code').fill(guest.code);
    await page.locator('#btn-verify').click();
    await expect(page.locator('#nav-user-dropdown')).toBeVisible();
    expect(psql(`SELECT is_verified FROM "user" WHERE id = '${guest.id}';`)).toBe('t');
  });
});
