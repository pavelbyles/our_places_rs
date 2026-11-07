import { test, expect } from '../fixtures/base.fixture.js';
import { createGuest, loginGuest, psql } from '../fixtures/bookings-data.js';
import { farStay, hostPage, listingInMode, requestStatus, submitRequest } from '../fixtures/requests-data.js';

test.describe('Guest Portal - Enquiries and requests to book (#105)', () => {
  test('a request-only listing asks for a request, which then waits for the host', async ({ page }) => {
    const villa = listingInMode('request');
    const stay = farStay();
    await loginGuest(page, createGuest());

    await page.goto(`/listings/${villa.slug}?check_in=${stay.checkIn}&check_out=${stay.checkOut}&guests=2`);
    await expect(page.locator('#listing-book-cta')).toHaveText('Request to book');
    await expect(page.locator('#listing-approval-badge')).toBeVisible();

    await page.goto(`/checkout/${villa.slug}?check_in=${stay.checkIn}&check_out=${stay.checkOut}&guests=2`);
    await expect(page.locator('#checkout-submit-btn')).toHaveCount(0);
    await expect(page.locator('#checkout-mode-note')).toContainText("isn't charged");

    const id = await submitRequest(page, villa.slug, stay, 'Celebrating a birthday');
    await expect(page.locator('#request-status')).toHaveText('Awaiting host');
    await expect(page.locator('#request-expiry')).toBeVisible();
    await expect(page.locator('#messages-list')).toContainText('Celebrating a birthday');
    expect(requestStatus(id)).toBe('open');

    await page.goto('/requests');
    await expect(page.locator(`#request-${id}`)).toContainText('Awaiting host');
  });

  test('the host accepts and the guest proceeds to a pending booking', async ({ page, browser }) => {
    const villa = listingInMode('request');
    const stay = farStay();
    const guest = createGuest();
    await loginGuest(page, guest);
    const id = await submitRequest(page, villa.slug, stay);

    const host = await hostPage(browser, villa.host);
    await host.goto(`/admin/requests/${id}`);
    await host.locator('#request-accept').click();
    await expect(host.locator('#request-notice')).toContainText('Accepted');
    await host.context().close();

    await page.goto(`/requests/${id}`);
    await expect(page.locator('#request-status')).toHaveText('Accepted');
    await page.locator('#request-checkout').click();
    await expect(page).toHaveURL(/\/bookings\?new_booking=true$/);
    expect(requestStatus(id)).toBe('converted');
    const row = psql(`SELECT status, date_from FROM booking WHERE guest_id = '${guest.id}' ORDER BY created_at DESC LIMIT 1;`);
    expect(row).toBe(`pending|${stay.checkIn}`);
  });

  test('a custom quote shows its terms and becomes a hold at the quoted total', async ({ page, browser }) => {
    const villa = listingInMode('request');
    const stay = farStay();
    const guest = createGuest();
    await loginGuest(page, guest);
    const id = await submitRequest(page, villa.slug, stay);

    const host = await hostPage(browser, villa.host);
    await host.goto(`/admin/requests/${id}`);
    await host.locator('#quote-stay-price').fill('1500');
    await host.locator('#quote-terms').fill('Wedding allowed until 10pm.');
    await host.locator('#quote-send').click();
    await expect(host.locator('#request-notice')).toContainText('Quote sent');
    await host.context().close();

    await page.goto(`/requests/${id}`);
    await expect(page.locator('#request-quote-terms')).toHaveText('Wedding allowed until 10pm.');
    const shownTotal = (await page.locator('#breakdown-total').innerText()).replaceAll(',', '');
    await page.locator('#request-accept-quote').click();
    await expect(page).toHaveURL(/\/bookings\?new_booking=true$/);
    const stored = psql(`SELECT total_price FROM booking WHERE guest_id = '${guest.id}' ORDER BY created_at DESC LIMIT 1;`);
    expect(shownTotal).toContain(stored);
  });

  test('a declined request shows the reason and frees the dates', async ({ page, browser }) => {
    const villa = listingInMode('request');
    const stay = farStay();
    await loginGuest(page, createGuest());
    const id = await submitRequest(page, villa.slug, stay);

    const host = await hostPage(browser, villa.host);
    await host.goto(`/admin/requests/${id}`);
    await host.locator('#decline-reason').selectOption('event_not_permitted');
    await host.locator('#decline-note').fill('No parties, sorry.');
    await host.locator('#request-decline-submit').click();
    await expect(host.locator('#request-notice')).toContainText('Declined');
    await host.context().close();

    await page.goto(`/requests/${id}`);
    await expect(page.locator('#request-decline')).toContainText("Events aren't permitted");
    await expect(page.locator('#request-decline')).toContainText('No parties, sorry.');
    const blocking = psql(`SELECT COUNT(*) FROM booking_request WHERE listing_id = '${villa.id}'
                           AND status IN ('open','accepted','quoted');`);
    expect(blocking).toBe('0');
  });

  test('an enquiry thread continues into a request and its booking', async ({ page, browser }) => {
    const villa = listingInMode('both');
    const stay = farStay();
    const guest = createGuest();
    await loginGuest(page, guest);

    await page.goto(`/listings/${villa.slug}?check_in=${stay.checkIn}&check_out=${stay.checkOut}&guests=2`);
    await page.locator('#listing-enquire-cta').click();
    await expect(page.locator('#enquiry-check-in')).toHaveValue(stay.checkIn);
    await page.locator('#enquiry-message').fill('Is the pool heated?');
    await page.locator('#enquiry-submit').click();
    await page.waitForURL(/\/requests\/enquiries\/[0-9a-f-]{36}$/);
    const enquiryId = page.url().split('/').pop() ?? '';

    const host = await hostPage(browser, villa.host);
    await host.goto(`/admin/enquiries/${enquiryId}`);
    await host.locator('#admin-message-text').fill('Yes, all year round.');
    await host.locator('#admin-message-text').press('Enter');
    await expect(host.locator('#admin-messages-list')).toContainText('Yes, all year round.');
    await host.context().close();

    await page.reload();
    await expect(page.locator('#messages-list')).toContainText('Yes, all year round.');
    await page.locator('#enquiry-request-cta').click();
    await expect(page.locator('input[name="enquiry_id"]')).toHaveValue(enquiryId);
    await page.locator('#checkout-request-btn').click();
    await page.waitForURL(/\/requests\/[0-9a-f-]{36}$/);
    await expect(page.locator('#messages-list')).toContainText('Is the pool heated?');
    expect(psql(`SELECT closed_reason FROM booking_enquiry WHERE id = '${enquiryId}';`)).toBe('request_submitted');
  });

  test('both mode offers booking now or requesting, and booking now still works', async ({ page }) => {
    const villa = listingInMode('both');
    const stay = farStay();
    const guest = createGuest();
    await loginGuest(page, guest);
    await page.goto(`/checkout/${villa.slug}?check_in=${stay.checkIn}&check_out=${stay.checkOut}&guests=2`);
    await expect(page.locator('#checkout-submit-btn')).toBeVisible();
    await expect(page.locator('#checkout-request-btn')).toHaveText('Request to book instead');
    await page.locator('#guest-phone').fill('+18765550100');
    await page.locator('#checkout-submit-btn').click();
    await expect(page).toHaveURL(/\/bookings\?new_booking=true$/);
  });
});
