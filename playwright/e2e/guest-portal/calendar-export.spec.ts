import { test, expect } from '../fixtures/base.fixture.js';
import { createGuest, createListingCopy, psql, signTestJwt } from '../fixtures/bookings-data.js';
import { fixtureDate } from '../setup/ical-fixture-server.js';

// iCal calendar sync (#58): the public, busy-only export feed, and blocked
// nights on the guest side.
const NEW_ONLY = { tag: '@new-only' };
const BOOKING_API_URL = process.env.BOOKING_API_URL || 'http://127.0.0.1:8081';

test.describe('Guest Portal - Calendar Export', () => {
  test('serves a busy-only .ics with reservation numbers and no guest details', NEW_ONLY, async ({ request }) => {
    const listing = createListingCopy();
    const guest = createGuest();
    const code = `EXP${guest.id.slice(0, 7).toUpperCase()}`;
    psql(`
      INSERT INTO booking (confirmation_code, guest_id, listing_id, status, date_from, date_to,
          currency, daily_rate, number_of_persons, total_days, sub_total_price, total_price,
          cancellation_policy)
      VALUES ('${code}', '${guest.id}', '${listing.id}', 'confirmed', '${fixtureDate(30)}', '${fixtureDate(33)}',
          'USD', 100, 1, 3, 300, 300, 'flexible');`);

    const res = await request.get(`/listings/${listing.id}/calendar.ics`);
    expect(res.status()).toBe(200);
    expect(res.headers()['content-type']).toContain('text/calendar');
    expect(res.headers()['etag']).toBeTruthy();
    const body = await res.text();
    expect(body).toContain('BEGIN:VCALENDAR');
    expect(body).toContain(`Reservation #: ${code}`);
    expect(body).not.toContain(guest.email);
    expect(body).not.toContain('Casey');

    const missing = await request.get(`/listings/${crypto.randomUUID()}/calendar.ics`);
    expect(missing.status()).toBe(404);
  });

  test('greys out blocked nights and refuses a hold on them', NEW_ONLY, async ({ page, request }) => {
    const listing = createListingCopy();
    const from = fixtureDate(450);
    const to = fixtureDate(452);
    psql(`INSERT INTO listing_blocked_dates (listing_id, source, date_from, date_to)
          VALUES ('${listing.id}', 'manual', '${from}', '${to}');`);

    await page.goto(`/listings/${listing.slug}/calendar?month=${from.slice(0, 7)}`);
    const night = page.locator(`button[data-date="${from}"]`);
    await expect(night).toHaveAttribute('aria-disabled', 'true');
    await expect(night).not.toHaveAttribute('data-state', 'available');

    const guest = createGuest();
    const hold = await request.post(`${BOOKING_API_URL}/api/v1/bookings`, {
      headers: { Authorization: `Bearer ${signTestJwt(guest.id)}` },
      data: {
        guest_id: guest.id, listing_id: listing.id, check_in: from, check_out: to,
        num_adults: 1, num_children: 0, num_infants: 0, num_pets: 0, message_to_host: null,
        estimated_arrival_time: null, is_business_trip: false, currency: 'USD',
        agreed_cancellation_policy: 'Flexible',
      },
    });
    expect(hold.status()).toBe(409);
    expect((await hold.json()).code).toBe('DATE_UNAVAILABLE');
  });
});
