import { test, expect, type Page } from '@playwright/test';
import * as fs from 'node:fs';
import * as path from 'node:path';

/**
 * ui.js calendar behaviour on markup rendered by the real
 * date_range_calendar component (fixtures/calendar.html is kept in sync by
 * web_ui_common/tests/date_range_calendar.rs). January 2027, today = Jan 5,
 * nights Jan 10-13 booked.
 */
const UI_JS = path.resolve(__dirname, '../../../web_ui_common/src/assets/ui.js');
const CALENDAR = fs.readFileSync(path.resolve(__dirname, 'fixtures/calendar.html'), 'utf8');

async function load(page: Page): Promise<void> {
  await page.goto('about:blank');
  await page.setContent(`<html><body><form id="quote">${CALENDAR}</form></body></html>`);
  await page.evaluate(() => {
    (window as unknown as { changes: number }).changes = 0;
    document.addEventListener('change', (e) => {
      if ((e.target as Element).id === 'booking-dates') (window as unknown as { changes: number }).changes += 1;
    });
  });
  await page.addScriptTag({ path: UI_JS });
}

const day = (page: Page, iso: string) => page.locator(`button[data-date="${iso}"]`);
/** Booked and past days are aria-disabled; people can still click them. */
const clickDay = (page: Page, iso: string) => day(page, iso).click({ force: true });
const checkIn = (page: Page) => page.locator('input[name="check_in"]');
const checkOut = (page: Page) => page.locator('input[name="check_out"]');
const status = (page: Page) => page.locator('[data-calendar-status]');

test.describe('ui.js calendar', () => {
  test('enhances the calendar with a single tab stop', async ({ page }) => {
    await load(page);
    await expect(page.locator('#booking-dates[data-enhanced]')).toHaveCount(1);
    await expect(page.locator('button[data-date][tabindex="0"]')).toHaveCount(1);
  });

  test('selects a range and reports the change', async ({ page }) => {
    await load(page);
    await clickDay(page, '2027-01-15');
    await expect(checkIn(page)).toHaveValue('2027-01-15');
    await expect(status(page)).toContainText('Choose a check-out');
    await clickDay(page, '2027-01-18');
    await expect(checkOut(page)).toHaveValue('2027-01-18');
    await expect(page.locator('button[data-in-range]')).toHaveCount(2);
    await expect(page.locator('td[aria-selected="true"]')).toHaveCount(2);
    expect(await page.evaluate(() => (window as unknown as { changes: number }).changes)).toBe(2);
  });

  test('rejects a range across booked nights', async ({ page }) => {
    await load(page);
    await clickDay(page, '2027-01-06');
    await clickDay(page, '2027-01-12');
    await expect(checkOut(page)).toHaveValue('');
    await expect(status(page)).toContainText('booked nights');
  });

  test('allows checking out on the first booked night', async ({ page }) => {
    await load(page);
    await clickDay(page, '2027-01-07');
    await clickDay(page, '2027-01-10');
    await expect(checkOut(page)).toHaveValue('2027-01-10');
  });

  test('ignores past and booked nights as check-in', async ({ page }) => {
    await load(page);
    await clickDay(page, '2027-01-03');
    await clickDay(page, '2027-01-11');
    await expect(checkIn(page)).toHaveValue('');
    await expect(status(page)).toContainText('not available');
  });

  test('moves with the keyboard and selects with Enter', async ({ page }) => {
    await load(page);
    await day(page, '2027-01-15').focus();
    await page.keyboard.press('ArrowRight');
    await expect(day(page, '2027-01-16')).toBeFocused();
    await page.keyboard.press('ArrowDown');
    await expect(day(page, '2027-01-23')).toBeFocused();
    await page.keyboard.press('Home');
    await expect(day(page, '2027-01-17')).toBeFocused(); // Sunday of that week
    await page.keyboard.press('PageDown');
    await expect(day(page, '2027-02-17')).toBeFocused();
    await expect(day(page, '2027-02-17')).toHaveAttribute('tabindex', '0');
    await page.keyboard.press('Enter');
    await expect(checkIn(page)).toHaveValue('2027-02-17');
  });
});
