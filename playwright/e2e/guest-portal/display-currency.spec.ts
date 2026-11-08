import { test, expect } from '../fixtures/base.fixture.js';
import { createGuest, loginGuest, psql } from '../fixtures/bookings-data.js';
import { KNOWN_LISTINGS } from '../fixtures/test-data.js';

const selector = (code: string) => `summary[aria-label="Display currency: ${code}"]`;

async function chooseCurrency(page: import('@playwright/test').Page, code: string) {
  await page.locator('summary[aria-label^="Display currency"], summary[aria-label^="Choose a display"]').click();
  await page.locator(`a[href^="/currency/${code}"]`).click();
  await expect(page.locator(selector(code))).toBeVisible();
}

test.describe('Guest Portal - Display currency', () => {
  const villa = KNOWN_LISTINGS.courtyardStudio;

  test('the chosen currency follows the guest to every page', async ({ page }) => {
    await page.goto('/');
    await chooseCurrency(page, 'JMD');
    await expect(page).toHaveURL(/\/$/);

    // No ?currency= in these URLs: the choice is remembered.
    await page.goto(`/listings/${villa.slug}`);
    await expect(page.locator(selector('JMD'))).toBeVisible();
    await expect(page.locator('#currency-notice')).toHaveCount(0);
    await page.goto('/listings');
    await expect(page.locator(selector('JMD'))).toBeVisible();

    // Choosing again replaces it.
    await chooseCurrency(page, 'USD');
    await page.goto(`/listings/${villa.slug}`);
    await expect(page.locator(selector('USD'))).toBeVisible();
  });

  test('a currency with no exchange rate shows the listing in its own currency, not a 404', async ({ page }) => {
    const noRate = psql(`
      SELECT c.code FROM currencies c
      WHERE c.active AND c.code <> 'USD'
        AND NOT EXISTS (SELECT 1 FROM currency_exchange_rates r
                        WHERE r.base_currency = 'USD' AND r.target_currency = c.code)
      ORDER BY c.code LIMIT 1;`);
    test.skip(!noRate, 'every offered currency has a rate');

    await page.goto('/');
    await chooseCurrency(page, noRate);
    const response = await page.goto(`/listings/${villa.slug}`);
    expect(response?.status()).toBe(200);
    await expect(page.locator('h1')).toHaveText(villa.name);
    await expect(page.locator('#currency-notice')).toContainText(`Prices in ${noRate} aren't available right now`);

    // An explicit ?currency= link behaves the same.
    const linked = await page.goto(`/listings/${villa.slug}?currency=${noRate}`);
    expect(linked?.status()).toBe(200);
    await expect(page.locator('#currency-notice')).toBeVisible();
  });

  test('signed in, the choice is saved to the profile, which applies when there is no cookie', async ({ page, context }) => {
    const guest = createGuest();
    psql(`UPDATE "user" SET default_currency = 'JMD' WHERE id = '${guest.id}';`);
    await loginGuest(page, guest);

    // Signing in with no cookie picks up the profile currency.
    await page.goto(`/listings/${villa.slug}`);
    await expect(page.locator(selector('JMD'))).toBeVisible();

    // A new choice is saved to the profile.
    await chooseCurrency(page, 'GBP');
    expect(psql(`SELECT default_currency FROM "user" WHERE id = '${guest.id}';`)).toBe('GBP');

    // With the cookie gone, the profile still applies.
    const cookies = await context.cookies();
    await context.clearCookies();
    await context.addCookies(cookies.filter((c) => c.name !== 'op_currency'));
    await page.goto('/listings');
    await expect(page.locator(selector('GBP'))).toBeVisible();

    // Saving the profile form changes the display currency too.
    await page.goto('/profile');
    await page.locator('#profile-currency').selectOption('JMD');
    await page.locator('form[action="/profile"] button[type="submit"]').click();
    await expect(page).toHaveURL(/\/profile\?saved=1$/);
    await expect(page.locator(selector('JMD'))).toBeVisible();
  });

  test('an unknown listing is still a 404', async ({ page }) => {
    const response = await page.goto('/listings/no-such-listing-e2e');
    expect(response?.status()).toBe(404);
  });

  test('the currency link only returns to pages on this site', async ({ page }) => {
    await page.goto('/currency/JMD?next=https://example.com/');
    await expect(page).toHaveURL(/^http:\/\/localhost:\d+\/$/);
    await expect(page.locator(selector('JMD'))).toBeVisible();
  });
});
