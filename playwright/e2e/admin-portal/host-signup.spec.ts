import { test, expect } from '../fixtures/base.fixture.js';
import { userVerificationCode } from '../fixtures/admin-data.js';
import { psql } from '../fixtures/bookings-data.js';

test.describe('Admin Portal - Host Sign-Up & Verification', () => {
  test('registers a host, verifies the emailed code and lands in the console', async ({ page }) => {
    const email = `e2e.newhost.${Date.now()}.${Math.floor(Math.random() * 1e6)}@example.com`;

    await page.goto('/register');
    await page.locator('#page-host-fn').fill('Morgan');
    await page.locator('#page-host-ln').fill('Signup');
    await page.locator('#page-host-email').fill(email);
    await page.locator('#page-host-phone').fill('+18765550123');
    await page.locator('#page-host-pass').fill('hostPassword123!');
    await page.locator('#btn-page-signup').click();

    await expect(page).toHaveURL(/\/verify\?email=/, { timeout: 15_000 });
    await expect.poll(() => userVerificationCode(email), { timeout: 10_000 }).not.toBe('');

    await page.locator('#verify-code').fill(userVerificationCode(email));
    await page.locator('#btn-verify').click();
    await expect(page).toHaveURL(/\/admin$/, { timeout: 15_000 });

    expect(psql(`SELECT is_verified FROM "user" WHERE email = '${email}';`)).toBe('t');
    expect(psql(`SELECT 'host' = ANY(roles) FROM "user" WHERE email = '${email}';`)).toBe('t');
  });

  test('rejects a wrong verification code', async ({ page }) => {
    const email = `e2e.newhost.${Date.now()}.${Math.floor(Math.random() * 1e6)}@example.com`;
    await page.goto('/register');
    await page.locator('#page-host-fn').fill('Riley');
    await page.locator('#page-host-ln').fill('Wrongcode');
    await page.locator('#page-host-email').fill(email);
    await page.locator('#page-host-pass').fill('hostPassword123!');
    await page.locator('#btn-page-signup').click();
    await expect(page).toHaveURL(/\/verify\?email=/, { timeout: 15_000 });

    await page.locator('#verify-code').fill('ZZZZZZ');
    await page.locator('#btn-verify').click();
    await expect(page.locator('#verify-error')).toBeVisible();
    expect(psql(`SELECT is_verified FROM "user" WHERE email = '${email}';`)).toBe('f');
  });
});
