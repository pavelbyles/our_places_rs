import { test, expect } from '../fixtures/base.fixture.js';
import { KNOWN_LISTINGS } from '../fixtures/test-data.js';

test.describe('Guest Portal - Listing Detail & Dynamic Pricing', () => {
  const targetVilla = KNOWN_LISTINGS.courtyardStudio;

  test('should display live listing details for The Courtyard Studio', async ({ page }) => {
    const response = await page.goto(`/listings/${targetVilla.slug}`);
    expect(response?.status()).toBe(200);

    // Fail if listing not found notice is displayed
    await expect(page.locator('text=Listing Not Found')).not.toBeVisible();

    // 1. Heading and villa title
    const heading = page.locator('h1');
    await expect(heading).toBeVisible();
    await expect(heading).toHaveText(targetVilla.name);

    // 2. Location and pricing badge
    await expect(page.locator('text=New Kingston, Jamaica').first()).toBeVisible();
    await expect(page.locator('#book').getByText('US$650')).toBeVisible();

    // 3. Amenities section
    await expect(page.locator('text=What this place offers')).toBeVisible();

    // 4. Reserve button linking to checkout
    const reserveBtn = page.locator(`a[href="/checkout/${targetVilla.slug}"]`).first();
    await expect(reserveBtn).toBeVisible();
    // The hold window is deploy configuration, so only the action is asserted.
    await expect(reserveBtn).toContainText('Reserve');

    // 5. Guest capacity dropdown with single-guest increments (1..=max_guests)
    const guestSelect = page.locator('select[name="guests"]');
    await expect(guestSelect).toBeVisible();
    const guestOptions = await guestSelect.locator('option').allInnerTexts();
    expect(guestOptions).toEqual(['1 Guest', '2 Guests', '3 Guests', '4 Guests']);
  });

  test('should recalculate dynamic price and statutory GCT on date selection', async ({ page }) => {
    const response = await page.goto(`/listings/${targetVilla.slug}`);
    expect(response?.status()).toBe(200);

    const checkInInput = page.locator('#reserve-check-in');
    const checkOutInput = page.locator('#reserve-check-out');

    await expect(checkInInput).toBeVisible();
    await expect(checkOutInput).toBeVisible();

    // Set 5-night stay dates starting from a known future date
    const now = new Date();
    const inDate = new Date(now.getTime() + 2 * 86400000);
    const outDate = new Date(inDate.getTime() + 5 * 86400000);

    const inStr = inDate.toISOString().split('T')[0];
    const outStr = outDate.toISOString().split('T')[0];

    await checkInInput.fill(inStr);
    await checkOutInput.fill(outStr);
    await checkOutInput.dispatchEvent('change');

    // Verify quote breakdown updates
    const breakdown = page.locator('#quote-breakdown');
    await expect(breakdown).toBeVisible();

    // Verify statutory GCT (15%) is present in breakdown
    await expect(breakdown).toContainText(/15%|GCT|Tax/i);
    await expect(breakdown).toContainText(/5 night/i);
  });
});

test.describe('Guest Portal - Luxury listing page (#122)', () => {
  const villa = KNOWN_LISTINGS.courtyardStudio;
  const url = `/listings/${villa.slug}`;

  test('shows every section the host filled in, in order', { tag: '@new-only' }, async ({ page }) => {
    await page.goto(url);
    await expect(page.getByTestId('listing-hero').locator('h1')).toHaveText(villa.name);
    const nav = page.getByRole('navigation', { name: 'On this page' });
    await expect(nav.locator('[data-section-link]')).toHaveText([
      'Overview', 'Suites', 'Amenities', 'Services', 'Policies', 'Reviews', 'Location', 'Photos',
    ]);
    await expect(page.locator('#overview-heading')).toHaveText('A private garden hideaway in the heart of New Kingston');

    const ribbon = page.getByTestId('highlights-ribbon');
    await expect(ribbon.locator('li')).toHaveCount(5);
    await expect(ribbon.locator('li').nth(4)).toContainText('Daily');

    await expect(page.getByTestId('suites').locator('[data-suite]')).toHaveCount(2);
    await expect(page.locator('[data-suite]').first()).toContainText('En-suite, rain shower');
    await expect(page.getByTestId('amenity-groups').locator('[data-amenity-group]')).toHaveCount(3);
    await expect(page.getByTestId('services-addons')).toContainText('US$120 each way');
    await expect(page.getByTestId('services-addons')).toContainText('On request');
    await expect(page.getByTestId('house-policies')).toContainText('From 3:00 PM');
    await expect(page.getByTestId('guestbook')).toContainText('Danielle');
    await expect(page.getByTestId('guestbook-notice')).toContainText('From the villa guestbook');
    await expect(page.getByTestId('drive-times')).toContainText('30 min');
    await expect(page.getByTestId('plan-your-stay')).toBeVisible();
    await expect(page.getByTestId('site-footer').locator('a[href="/terms"]')).toBeVisible();
  });

  test('an add-on opens the enquiry with the service named', { tag: '@new-only' }, async ({ page }) => {
    await page.goto(url);
    const link = page.getByTestId('services-addons').getByRole('link', { name: 'Airport transfer, KIN' });
    await expect(link).toHaveAttribute('href', /\/enquire\?topic=Airport%20transfer%2C%20KIN$/);
  });

  test('phones get a two-column ribbon and a booking bar, with no sideways scroll', { tag: '@new-only' }, async ({ page }) => {
    await page.setViewportSize({ width: 390, height: 844 });
    await page.goto(url);
    const overflow = await page.evaluate(() => document.documentElement.scrollWidth - document.documentElement.clientWidth);
    expect(overflow).toBe(0);
    const columns = await page
      .getByTestId('highlights-ribbon')
      .locator('ul')
      .evaluate((ul) => getComputedStyle(ul).gridTemplateColumns.split(' ').length);
    expect(columns).toBe(2);
    await expect(page.locator('[data-mobile-book-bar]')).toBeVisible();
  });

  test('wide screens keep the booking card beside the content and hide the bar', { tag: '@new-only' }, async ({ page }) => {
    await page.setViewportSize({ width: 1280, height: 900 });
    await page.goto(url);
    await expect(page.locator('[data-mobile-book-bar]')).toBeHidden();
    expect(await page.locator('#book').evaluate((el) => getComputedStyle(el).position)).toBe('sticky');
    await page.locator('#location').scrollIntoViewIfNeeded();
    const navTop = await page.locator('[data-section-nav]').evaluate((el) => el.getBoundingClientRect().top);
    expect(navTop).toBe(0);
  });

  for (const theme of ['', 'dim', 'dark']) {
    test(`the ribbon reads clearly in the ${theme || 'light'} theme`, { tag: '@new-only' }, async ({ page }) => {
      await page.goto(url);
      await page.evaluate((t) => {
        document.documentElement.classList.remove('dim', 'dark');
        if (t) document.documentElement.classList.add(t);
      }, theme);
      const [bg, fg] = await page
        .getByTestId('highlights-ribbon')
        .evaluate((el) => [getComputedStyle(el).backgroundColor, getComputedStyle(el).color]);
      expect(bg).not.toBe(fg);
      expect(bg).not.toBe('rgba(0, 0, 0, 0)');
    });
  }

  test('other guest pages keep their layout and never scroll sideways', async ({ page }) => {
    await page.setViewportSize({ width: 390, height: 844 });
    for (const path of ['/', '/listings', '/about', '/terms']) {
      await page.goto(path);
      const overflow = await page.evaluate(() => document.documentElement.scrollWidth - document.documentElement.clientWidth);
      expect(overflow, path).toBe(0);
      const left = await page.locator('main#main > *').first().evaluate((el) => el.getBoundingClientRect().left);
      expect(left, path).toBeGreaterThanOrEqual(24);
    }
  });
});

