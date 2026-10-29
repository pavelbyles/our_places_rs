import { test, expect } from '../fixtures/base.fixture.js';

// Spec FR-3.6 / D7: light → dim → dark toggle, remembered in op_theme; with
// no saved choice, dark at or after 18:00 local time (visitor offset from
// op_tz_offset, Jamaica UTC-05:00 before that). Rendered on the server.
const NEW_ONLY = { tag: '@new-only' };

/** An op_tz_offset (minutes east of UTC) that makes the local hour `hour`. */
function offsetForLocalHour(hour: number): number {
  const utc = new Date().getUTCHours();
  let delta = (hour - utc + 24) % 24; // 0..23 hours ahead
  if (delta > 14) delta -= 24; // keep within -12..+14
  return delta * 60;
}

async function htmlClass(page: import('@playwright/test').Page): Promise<string> {
  return (await page.locator('html').getAttribute('class')) ?? '';
}

test.describe('Guest Portal - Theme', () => {
  test('renders the theme on the server and cycles light, dim, dark', NEW_ONLY, async ({ page, context }) => {
    await context.addCookies([{ name: 'op_theme', value: 'light', url: 'http://localhost:3010' }]);
    const res = await page.goto('/');
    expect(await res?.text()).toMatch(/<html lang="en" class="">/);

    const toggle = page.locator('[data-theme-cycle]').first();
    await expect(toggle).toHaveAttribute('aria-label', 'Switch to dim theme');
    await toggle.click();
    await expect(page.locator('html')).toHaveClass('dim');

    const reload = await page.reload();
    expect(await reload?.text()).toContain('<html lang="en" class="dim">');
    await page.locator('[data-theme-cycle]').first().click();
    await page.reload();
    expect(await htmlClass(page)).toBe('dark');
  });

  test('defaults to dark in the evening and light by day, local time', NEW_ONLY, async ({ browser }) => {
    for (const [hour, expected] of [[20, 'dark'], [10, '']] as const) {
      const context = await browser.newContext();
      await context.addCookies([
        { name: 'op_tz_offset', value: String(offsetForLocalHour(hour)), url: 'http://localhost:3010' },
      ]);
      const page = await context.newPage();
      const res = await page.request.get('http://localhost:3010/');
      expect(await res.text(), `local ${hour}:00`).toContain(`<html lang="en" class="${expected}">`);
      await context.close();
    }
  });

  test('uses Jamaica time on a first visit and then stores the offset', NEW_ONLY, async ({ browser }) => {
    const context = await browser.newContext({ timezoneId: 'Asia/Tokyo' });
    const page = await context.newPage();
    const jamaicaHour = (new Date().getUTCHours() + 24 - 5) % 24;
    const res = await page.goto('http://localhost:3010/');
    expect(await res?.text()).toContain(`<html lang="en" class="${jamaicaHour >= 18 ? 'dark' : ''}">`);
    const cookies = await context.cookies();
    expect(cookies.find((c) => c.name === 'op_tz_offset')?.value).toBe('540');
    await context.close();
  });
});
