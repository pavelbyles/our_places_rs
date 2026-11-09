import { test, expect } from '../fixtures/auth.fixture.js';
import { createGuest, createListingCopy, psql } from '../fixtures/bookings-data.js';
import { ICAL_FIXTURE_BASE, fixtureDate } from '../setup/ical-fixture-server.js';

// iCal calendar sync (#58): import a channel's feed, sync it, see its blocks
// and the conflict it causes, and block dates by hand.
const NEW_ONLY = { tag: '@new-only' };

/** A confirmed stay on `listingId` inside the fixture's first event (days +400..+404). */
function confirmedStayInsideFixture(listingId: string): string {
  const guest = createGuest();
  const code = `E2E${guest.id.slice(0, 7).toUpperCase()}`;
  psql(`
    INSERT INTO booking (confirmation_code, guest_id, listing_id, status, date_from, date_to,
        currency, daily_rate, number_of_persons, total_days, sub_total_price, total_price,
        cancellation_policy)
    VALUES ('${code}', '${guest.id}', '${listingId}', 'confirmed', '${fixtureDate(401)}', '${fixtureDate(403)}',
        'USD', 100, 1, 2, 200, 200, 'flexible');`);
  return code;
}

test.describe('Admin Portal - Calendar Sync', () => {
  test('imports a feed, rate limits Sync now, shows blocks and acknowledges the conflict', NEW_ONLY, async ({ authenticatedAdminPage: page }) => {
    const listing = createListingCopy();
    const code = confirmedStayInsideFixture(listing.id);
    await page.goto(`/admin/listings/${listing.id}/calendar`);
    await expect(page.locator('#calendar-export-url')).toHaveValue(new RegExp(`/listings/${listing.id}/calendar\\.ics$`));

    const add = page.locator(`form[action$="/calendar/feeds"]`);
    await add.locator('input[name="name"]').fill('E2E Airbnb');
    await add.locator('input[name="url"]').fill(`${ICAL_FIXTURE_BASE}/airbnb.ics`);
    await add.locator('[type="submit"]').click();
    await expect(page.locator('#calendar-feeds tr[data-status="ok"]', { hasText: 'E2E Airbnb' })).toBeVisible({ timeout: 25_000 });
    await expect(page.locator('#calendar-sync-results')).toContainText('synced');

    // The first sync just ran, so "Sync now" is rate limited for a minute.
    await page.locator('#calendar-sync-now').click();
    await expect(page.locator('#calendar-feeds [role="alert"]')).toContainText('Synced less than a minute ago');

    await page.reload();
    await expect(page.locator('#calendar-blocks')).toContainText('From E2E Airbnb');
    const conflict = page.locator('#open-conflicts li', { hasText: code });
    await expect(conflict).toBeVisible();
    await conflict.getByRole('button', { name: 'Acknowledge' }).click();
    await expect(page.locator('#open-conflicts')).toContainText('No open conflicts.');
    await expect(page.locator('#conflict-history summary')).toContainText('Acknowledged (1)');
  });

  test('adds and removes a manual block', NEW_ONLY, async ({ authenticatedAdminPage: page }) => {
    const listing = createListingCopy();
    await page.goto(`/admin/listings/${listing.id}/calendar`);
    const form = page.locator(`form[action$="/calendar/blocks"]`);
    await form.locator('input[name="date_from"]').fill(fixtureDate(500));
    await form.locator('input[name="date_to"]').fill(fixtureDate(502));
    await form.locator('input[name="note"]').fill('Owner stay');
    await form.locator('[type="submit"]').click();
    const row = page.locator('#manual-blocks li', { hasText: 'Owner stay' });
    await expect(row).toBeVisible();
    await row.getByRole('button', { name: 'Remove' }).click();
    await expect(page.locator('#manual-blocks')).toContainText('No dates blocked by hand.');
  });
});
