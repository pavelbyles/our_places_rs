import { randomInt } from 'node:crypto';
import { test, expect } from '../fixtures/base.fixture.js';
import {
  createBooking,
  createGuest,
  createListingCopy,
  loginGuest,
  psql,
} from '../fixtures/bookings-data.js';

// Search by dates, guests and place (#101).
const iso = (d: Date) => d.toISOString().slice(0, 10);
const plusDays = (date: string, days: number) =>
  iso(new Date(Date.parse(`${date}T00:00:00Z`) + days * 86_400_000));

/** A random 5-night stay far enough ahead to be free of other bookings. */
function freeStay(): { checkIn: string; checkOut: string } {
  const checkIn = iso(new Date(Date.UTC(2038, 0, 1 + randomInt(1000))));
  return { checkIn, checkOut: plusDays(checkIn, 5) };
}

// "USD $1,234.50" (card or breakdown) gives "USD 1234.50".
const amountIn = (text: string) => {
  const m = /([A-Z]{3}) [^\d-]*(-?[\d,]+\.\d{2})/.exec(text);
  return m ? `${m[1]} ${m[2].replaceAll(',', '')}` : '';
};

test.describe('Guest Portal - Search by dates and guests', () => {
  test('a home search reaches /listings with its filters, and reloads the same', async ({ page }) => {
    const copy = createListingCopy();
    const other = createListingCopy();
    const { checkIn, checkOut } = freeStay();

    await page.goto('/');
    const form = page.locator('form[aria-label="Search villas"]');
    await form.locator('input[name="q"]').fill(copy.name);
    await form.locator('input[name="check_in"]').fill(checkIn);
    await form.locator('input[name="check_out"]').fill(checkOut);
    await form.locator('input[name="guests"]').fill('2');
    await form.getByRole('button', { name: 'Search villas' }).click();

    await expect(page).toHaveURL(new RegExp(`/listings\\?.*check_in=${checkIn}`));
    await expect(page).toHaveURL(new RegExp(`check_out=${checkOut}`));
    const card = page.locator(`#listing-card-${copy.slug}`);
    await expect(card).toBeVisible();
    await expect(card.locator('[data-stay-total]')).toContainText('total for 5 nights');
    await expect(page.locator(`#listing-card-${other.slug}`)).toHaveCount(0);

    const total = await card.locator('[data-stay-total]').innerText();
    await page.reload();
    expect(await page.locator(`#listing-card-${copy.slug} [data-stay-total]`).innerText()).toBe(total);
    await expect(page.locator('#search-check-in')).toHaveValue(checkIn);
    await expect(page.locator('#search-check-out')).toHaveValue(checkOut);
    await expect(page.locator('#search-guests')).toHaveValue('2');
    await expect(page.locator(`#listing-card-${other.slug}`)).toHaveCount(0);
  });

  test('a card link keeps the stay through to checkout, at the same total', async ({ page }) => {
    const guest = createGuest();
    await loginGuest(page, guest);
    const copy = createListingCopy();
    const { checkIn, checkOut } = freeStay();

    await page.goto(
      `/listings?q=${encodeURIComponent(copy.name)}&check_in=${checkIn}&check_out=${checkOut}&guests=2`,
    );
    const card = page.locator(`#listing-card-${copy.slug}`);
    const cardTotal = amountIn(await card.locator('[data-stay-total]').innerText());
    expect(cardTotal).not.toBe('');

    await card.getByRole('heading').getByRole('link').click();
    await expect(page).toHaveURL(
      new RegExp(`/listings/${copy.slug}\\?check_in=${checkIn}&check_out=${checkOut}&guests=2`),
    );
    expect(amountIn(await page.locator('#breakdown-total').innerText())).toBe(cardTotal);

    await page.locator('#quote a[data-listing]').click();
    await expect(page).toHaveURL(
      new RegExp(`/checkout/${copy.slug}\\?check_in=${checkIn}&check_out=${checkOut}&guests=2`),
    );
    expect(amountIn(await page.locator('#breakdown-total').innerText())).toBe(cardTotal);
  });

  test('switching currency keeps the search and changes the total', async ({ page }) => {
    const guest = createGuest();
    await loginGuest(page, guest);
    const copy = createListingCopy();
    const { checkIn, checkOut } = freeStay();
    const stay = `check_in=${checkIn}&check_out=${checkOut}&guests=2`;
    const switchToJmd = async () => {
      await page.locator('summary[aria-label^="Display currency"], summary[aria-label^="Choose a display"]').click();
      await page.locator('a[href^="?currency=JMD"]').click();
      await expect(page).toHaveURL(/currency=JMD/);
      await expect(page).toHaveURL(new RegExp(`check_in=${checkIn}`));
      await expect(page).toHaveURL(new RegExp(`check_out=${checkOut}`));
      await expect(page).toHaveURL(/guests=2/);
    };

    await page.goto(`/listings?q=${encodeURIComponent(copy.name)}&${stay}`);
    await switchToJmd();
    await expect(page).toHaveURL(/q=/);
    await expect(page.locator(`#listing-card-${copy.slug} [data-stay-total]`)).toContainText('JMD');

    await page.goto(`/listings/${copy.slug}?${stay}`);
    await switchToJmd();
    await expect(page.locator('#breakdown-total')).toContainText('JMD');

    await page.goto(`/checkout/${copy.slug}?${stay}`);
    await switchToJmd();
    await expect(page.locator('#breakdown-total')).toContainText('JMD');
  });

  test('the booking gap applies to search and the calendar', async ({ page, request }) => {
    // A booking mid-month, so every date clicked below is in the month shown:
    // the calendar hides its second month.
    let copy = createListingCopy();
    let booking = await createBooking(request, createGuest().id, {
      confirmed: true,
      listingId: copy.id,
    });
    for (let attempt = 0; attempt < 20 && !/-(09|1\d|20)$/.test(booking.checkIn); attempt++) {
      copy = createListingCopy();
      booking = await createBooking(request, createGuest().id, {
        confirmed: true,
        listingId: copy.id,
      });
    }
    psql(`UPDATE listing SET days_between_bookings = 2 WHERE id = '${copy.id}';`);
    const q = encodeURIComponent(copy.name);
    const search = (from: string) =>
      page.goto(`/listings?q=${q}&check_in=${from}&check_out=${plusDays(from, 3)}`);
    const card = page.locator(`#listing-card-${copy.slug}`);

    await search(plusDays(booking.checkOut, 1));
    await expect(card).toHaveCount(0);
    await expect(page.getByText('No stays match those dates')).toBeVisible();
    await search(plusDays(booking.checkOut, 2));
    await expect(card).toBeVisible();

    await page.goto(`/listings/${copy.slug}/calendar?month=${booking.checkOut.slice(0, 7)}`);
    await expect(page.locator(`button[data-date="${plusDays(booking.checkOut, 1)}"]`)).toHaveAttribute(
      'data-state',
      'booked',
    );
    await expect(page.locator(`button[data-date="${plusDays(booking.checkOut, 2)}"]`)).toHaveAttribute(
      'data-state',
      'available',
    );

    // The last check-out the gap allows can still be selected: the guest leaves
    // that morning.
    const lastCheckOut = plusDays(booking.checkIn, -2);
    const start = plusDays(lastCheckOut, -3);
    await page.goto(`/listings/${copy.slug}?check_in=${start}`);
    const calendar = page.locator('#booking-dates');
    await expect(calendar).toHaveAttribute('data-enhanced', /.*/);
    await calendar.locator(`button[data-date="${start}"]`).click();
    // `aria-disabled`, so Playwright won't click it unforced, but ui.js allows it.
    await calendar.locator(`button[data-date="${lastCheckOut}"]`).click({ force: true });
    await expect(page.locator('#reserve-check-out')).toHaveValue(lastCheckOut);
  });

  test('an old bookmark explains itself and offers new dates', async ({ page }) => {
    await page.goto('/listings?q=villa&check_in=2020-01-01&check_out=2020-01-05&guests=2');
    await expect(page.locator('#search-refused')).toHaveText(
      "Invalid date range: dates can't be in the past.",
    );
    await expect(page.locator('#listings-grid')).toHaveCount(0);
    await expect(page.getByText('stays across Jamaica')).toHaveCount(0);

    const { checkIn, checkOut } = freeStay();
    await page.locator('#listing-search').fill('');
    await page.locator('#search-check-in').fill(checkIn);
    await page.locator('#search-check-out').fill(checkOut);
    await page.locator('#listing-search-form').getByRole('button', { name: 'Search' }).click();
    await expect(page).toHaveURL(new RegExp(`check_in=${checkIn}`));
    await expect(page.locator('#search-refused')).toHaveCount(0);
    await expect(page.locator('#listings-grid')).toBeVisible();
  });

  test('text finds a listing by city and by country', async ({ page }) => {
    const copy = createListingCopy();
    const city = `E2E Bay ${copy.id}`;
    const country = `E2E Land ${copy.id}`;
    psql(`UPDATE listing SET city = '${city}', country = '${country}' WHERE id = '${copy.id}';`);
    const card = page.locator(`#listing-card-${copy.slug}`);

    await page.goto(`/listings?q=${encodeURIComponent(city.toLowerCase())}`);
    await expect(card).toBeVisible();
    await page.goto(`/listings?q=${encodeURIComponent(country.toUpperCase())}`);
    await expect(card).toBeVisible();
  });

  test('with no guest count, the listing page and checkout use one guest', async ({ page }) => {
    const guest = createGuest();
    await loginGuest(page, guest);
    const copy = createListingCopy();
    const { checkIn, checkOut } = freeStay();
    const stay = `check_in=${checkIn}&check_out=${checkOut}`;

    await page.goto(`/listings/${copy.slug}?${stay}`);
    await expect(page.locator('#reserve-form select[name="guests"]')).toHaveValue('1');
    const listingTotal = await page.locator('#breakdown-total').innerText();

    await page.goto(`/checkout/${copy.slug}?${stay}`);
    await expect(page.locator('#breakdown-total')).toHaveText(listingTotal);

    await page.goto(`/listings?q=${encodeURIComponent(copy.name)}&${stay}`);
    await expect(page.locator(`#listing-card-${copy.slug} a`).first()).toHaveAttribute(
      'href',
      new RegExp(`guests=1`),
    );
  });

  test('the search page fits a phone', async ({ page }) => {
    await page.setViewportSize({ width: 375, height: 700 });
    const { checkIn, checkOut } = freeStay();
    await page.goto(`/listings?check_in=${checkIn}&check_out=${checkOut}&guests=2`);
    await expect(page.locator('#listing-search-form')).toBeVisible();
    await expect(page.locator('#search-check-out')).toBeVisible();
    const overflow = await page.evaluate(
      () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
    );
    expect(overflow).toBeLessThanOrEqual(0);
  });
});
