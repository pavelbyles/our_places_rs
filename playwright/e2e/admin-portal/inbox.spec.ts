import { test, expect } from '../fixtures/base.fixture.js';
import { loginAdminAs } from '../fixtures/admin-data.js';
import { createGuest, psql } from '../fixtures/bookings-data.js';
import { farStay, listingInMode } from '../fixtures/requests-data.js';

/** The value a `RETURNING` query printed (psql also prints the command tag). */
function returned(sql: string): string {
  return psql(sql).split('\n')[0].trim();
}

/** Inserts an open request straight into the database, as a guest would. */
function seedRequest(listingId: string, guestId: string, stay: { checkIn: string; checkOut: string }): string {
  const snapshot = JSON.stringify({
    currency: 'USD', exchange_rate: '1', daily_rate: '650.00', total_days: 3,
    sub_total_price: '1950.00', discount_value: null, discounted_subtotal: '1950.00',
    cleaning_fee: '0.00', damage_deposit: '0.00', platform_fee: '97.50', tax_rate: '0.15',
    tax_value: '292.50', fee_breakdown: [], total_price: '2340.00', cancellation_policy: 'flexible', promo: null,
  });
  return returned(`
    WITH conv AS (
      INSERT INTO conversation (listing_id, guest_id) VALUES ('${listingId}', '${guestId}')
      ON CONFLICT (listing_id, guest_id) DO UPDATE SET last_message_at = NOW() RETURNING id
    )
    INSERT INTO booking_request (conversation_id, listing_id, guest_id, origin, date_from, date_to,
        number_of_persons, currency, price_snapshot, created_at, expires_at)
    SELECT conv.id, '${listingId}', '${guestId}', 'guest_request', '${stay.checkIn}', '${stay.checkOut}',
        2, 'USD', '${snapshot}'::jsonb, NOW() - INTERVAL '20 hours', NOW() + INTERVAL '4 hours'
    FROM conv RETURNING id;`);
}

test.describe('Admin Portal - Inbox (#105)', () => {
  test('shows a near-expiry request and the host can accept it', async ({ page }) => {
    const villa = listingInMode('request');
    const id = seedRequest(villa.id, createGuest().id, farStay());
    await loginAdminAs(page, villa.host);

    await page.goto('/admin/inbox');
    const row = page.locator(`#inbox-${id}`);
    await expect(row).toContainText('Request');
    await expect(row.locator('[data-near-expiry]')).toBeVisible();
    await expect(page.locator('#inbox-metrics')).toBeVisible();

    await row.locator('a').first().click();
    await page.locator('#request-accept').click();
    await expect(page.locator('#request-notice')).toContainText('Accepted');
    expect(psql(`SELECT status FROM booking_request WHERE id = '${id}';`)).toBe('accepted');
  });

  test('previews and sends a quote, and declines with a reason', async ({ page }) => {
    const villa = listingInMode('request');
    const id = seedRequest(villa.id, createGuest().id, farStay());
    await loginAdminAs(page, villa.host);
    await page.goto(`/admin/requests/${id}`);

    await page.locator('#quote-stay-price').fill('1,000');
    await page.locator('#quote-preview').click();
    await expect(page.locator('#quote-preview-result')).toContainText('number above zero');
    await page.locator('#quote-stay-price').fill('1500');
    await page.locator('#quote-preview').click();
    await expect(page.locator('#quote-preview-breakdown')).toContainText('2036.25');
    await page.locator('#quote-send').click();
    await expect(page.locator('#request-notice')).toContainText('Quote sent');
    await expect(page.locator('#request-quote')).toBeVisible();

    await page.locator('#decline-reason').selectOption('other');
    await page.locator('#request-decline-submit').click();
    await expect(page.locator('#request-notice')).toContainText('explain');
    await page.locator('#decline-note').fill('Renovations that week.');
    await page.locator('#request-decline-submit').click();
    await expect(page.locator('#request-notice')).toContainText('Declined');
  });

  test('quotes straight on an enquiry, and closes another', async ({ page }) => {
    const villa = listingInMode('both');
    const stay = farStay();
    const guest = createGuest();
    const enquiry = (conv: string) => returned(`
      INSERT INTO booking_enquiry (conversation_id, listing_id, guest_id, date_from, date_to, number_of_persons, currency)
      VALUES ('${conv}', '${villa.id}', '${guest.id}', '${stay.checkIn}', '${stay.checkOut}', 2, 'USD') RETURNING id;`);
    const conv = returned(`INSERT INTO conversation (listing_id, guest_id) VALUES ('${villa.id}', '${guest.id}') RETURNING id;`);
    const first = enquiry(conv);
    await loginAdminAs(page, villa.host);

    await page.goto(`/admin/enquiries/${first}`);
    await page.locator('#quote-stay-price').fill('1200');
    await page.locator('#quote-send').click();
    await page.waitForURL(/\/admin\/requests\/[0-9a-f-]{36}\?notice=quoted$/);
    await expect(page.locator('#request-status')).toHaveText('Quote received');

    const other = createGuest();
    const conv2 = returned(`INSERT INTO conversation (listing_id, guest_id) VALUES ('${villa.id}', '${other.id}') RETURNING id;`);
    const second = returned(`
      INSERT INTO booking_enquiry (conversation_id, listing_id, guest_id, date_from, date_to, number_of_persons, currency)
      VALUES ('${conv2}', '${villa.id}', '${other.id}', '${stay.checkIn}', '${stay.checkOut}', 2, 'USD') RETURNING id;`);
    await page.goto(`/admin/enquiries/${second}`);
    await page.locator('#enquiry-close').click();
    await expect(page.locator('#enquiry-notice')).toContainText('closed');
    await expect(page.locator('#enquiry-status')).toHaveText('Closed');
  });

  test('changes a listing to request-only from the listing form', async ({ page }) => {
    const villa = listingInMode('instant');
    await loginAdminAs(page, villa.host);
    await page.goto(`/admin/listings/${villa.id}/edit`);
    await page.locator('#listing-booking-mode').selectOption('request');
    await page.locator('#btn-save-listing').click();
    await expect.poll(() => psql(`SELECT booking_mode FROM listing WHERE id = '${villa.id}';`)).toBe('request');
  });
});
