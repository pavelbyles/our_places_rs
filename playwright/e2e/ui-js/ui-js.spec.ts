import { test, expect, type Page } from '@playwright/test';
import * as path from 'node:path';

/**
 * Unit-level browser tests for web_ui_common's ui.js, loaded into a bare
 * page. CSP compliance is covered by the portal e2e suites.
 */
const UI_JS = path.resolve(__dirname, '../../../web_ui_common/src/assets/ui.js');

const FIXTURE = `
<html><body>
  <button id="toggle" data-theme-cycle>Theme</button>
  <button id="open" data-dialog-open="dlg">Open</button>
  <dialog id="dlg"><p>Hello</p><button id="close" data-dialog-close>Close</button></dialog>
  <details id="menu" data-dismiss-outside><summary>Menu</summary><a href="#">Item</a></details>
  <p id="outside">Outside</p>
  <button id="print" data-print>Print</button>
  <div id="toast-region" aria-live="polite"></div>
</body></html>`;

async function load(page: Page, htmlClass = ''): Promise<void> {
  await page.goto('about:blank');
  await page.setContent(FIXTURE.replace('<html>', `<html class="${htmlClass}">`));
  await page.addScriptTag({ path: UI_JS });
}

test.describe('ui.js', () => {
  test('cycles light -> dim -> dark -> light and labels the next theme', async ({ page }) => {
    await load(page);
    const html = page.locator('html');
    const toggle = page.locator('#toggle');
    await expect(toggle).toHaveAttribute('aria-label', 'Switch to dim theme');

    await toggle.click();
    await expect(html).toHaveClass('dim');
    await expect(toggle).toHaveAttribute('aria-label', 'Switch to dark theme');
    await toggle.click();
    await expect(html).toHaveClass('dark');
    await toggle.click();
    await expect(html).not.toHaveClass(/dim|dark/);
    await expect(toggle).toHaveAttribute('aria-label', 'Switch to dim theme');
  });

  test('picks up a server-rendered theme', async ({ page }) => {
    await load(page, 'dark');
    await expect(page.locator('#toggle')).toHaveAttribute('aria-label', 'Switch to light theme');
  });

  test('opens and closes dialogs, including from the backdrop and Escape', async ({ page }) => {
    await load(page);
    const dialog = page.locator('#dlg');
    await page.locator('#open').click();
    await expect(dialog).toBeVisible();
    await page.locator('#close').click();
    await expect(dialog).toBeHidden();

    await page.locator('#open').click();
    await page.mouse.click(5, 5); // backdrop
    await expect(dialog).toBeHidden();

    await page.locator('#open').click();
    await page.keyboard.press('Escape');
    await expect(dialog).toBeHidden();
  });

  test('closes dropdowns on outside click and Escape', async ({ page }) => {
    await load(page);
    const menu = page.locator('#menu');
    await page.locator('#menu summary').click();
    await expect(menu).toHaveAttribute('open', '');
    await page.locator('#outside').click();
    await expect(menu).not.toHaveAttribute('open');

    await page.locator('#menu summary').click();
    await page.keyboard.press('Escape');
    await expect(menu).not.toHaveAttribute('open');
  });

  test('renders toasts as text, never HTML', async ({ page }) => {
    await load(page);
    await page.evaluate(() =>
      document.body.dispatchEvent(
        new CustomEvent('toast', { bubbles: true, detail: { message: '<img src=x onerror=alert(1)>', variant: 'success' } }),
      ),
    );
    const toast = page.locator('#toast-region [role="status"]');
    await expect(toast).toHaveText('<img src=x onerror=alert(1)>');
    await expect(toast).toHaveAttribute('data-variant', 'success');
    await expect(page.locator('#toast-region img')).toHaveCount(0);
  });

  test('print button calls window.print', async ({ page }) => {
    await load(page);
    await page.evaluate(() => {
      (window as unknown as { printed: number }).printed = 0;
      window.print = () => { (window as unknown as { printed: number }).printed += 1; };
    });
    await page.locator('#print').click();
    expect(await page.evaluate(() => (window as unknown as { printed: number }).printed)).toBe(1);
  });
});

test.describe('ui.js cookies', () => {
  test('stores the theme choice and time-zone offset', async ({ page, context }) => {
    // Cookies need a real origin; serve the fixture from a routed URL.
    await page.route('http://ui-js.test/', (route) => route.fulfill({ contentType: 'text/html', body: FIXTURE }));
    await page.goto('http://ui-js.test/');
    await page.addScriptTag({ path: UI_JS });
    await page.locator('#toggle').click();

    const cookies = Object.fromEntries((await context.cookies()).map((c) => [c.name, c]));
    expect(cookies.op_theme?.value).toBe('dim');
    expect(cookies.op_theme?.sameSite).toBe('Lax');
    const expectedOffset = await page.evaluate(() => String(-new Date().getTimezoneOffset()));
    expect(cookies.op_tz_offset?.value).toBe(expectedOffset);
  });
});

const GALLERY = `
<html><body>
  <div data-lightbox-group="viewer">
    <button id="p0" data-lightbox-index="0" data-full-src="/a.webp" data-alt="Pool">A</button>
    <button id="p1" data-lightbox-index="1" data-full-src="/b.webp" data-alt="Deck">B</button>
    <button id="p2" data-lightbox-index="2" data-full-src="/c.webp" data-alt="Bedroom">C</button>
  </div>
  <dialog id="viewer" data-lightbox>
    <img id="img" data-lightbox-img alt="">
    <button id="prev" data-lightbox-step="-1">Prev</button>
    <button id="next" data-lightbox-step="1">Next</button>
    <p id="counter" data-lightbox-counter></p>
    <button id="close" data-dialog-close>Close</button>
  </dialog>
  <textarea id="desc" maxlength="20"></textarea>
  <p id="desc-count" data-char-count="desc">0 / 20</p>
</body></html>`;

test.describe('ui.js photo viewer and counters', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('about:blank');
    await page.setContent(GALLERY);
    await page.addScriptTag({ path: UI_JS });
  });

  test('opens the clicked photo full size and steps through, wrapping', async ({ page }) => {
    const img = page.locator('#img');
    await page.locator('#p1').click();
    await expect(page.locator('#viewer')).toHaveAttribute('open', '');
    await expect(img).toHaveAttribute('src', '/b.webp');
    await expect(img).toHaveAttribute('alt', 'Deck');
    await expect(page.locator('#counter')).toHaveText('2 / 3');

    await page.locator('#next').click();
    await expect(img).toHaveAttribute('src', '/c.webp');
    await page.locator('#next').click();
    await expect(img).toHaveAttribute('src', '/a.webp');
    await expect(page.locator('#counter')).toHaveText('1 / 3');
    await page.locator('#prev').click();
    await expect(img).toHaveAttribute('src', '/c.webp');

    await page.keyboard.press('ArrowLeft');
    await expect(img).toHaveAttribute('src', '/b.webp');
    await page.keyboard.press('ArrowRight');
    await expect(img).toHaveAttribute('src', '/c.webp');

    await page.locator('#close').click();
    await expect(page.locator('#viewer')).not.toHaveAttribute('open', '');
  });

  test('counts characters against maxlength as the user types', async ({ page }) => {
    await page.locator('#desc').fill('Ocean view');
    await expect(page.locator('#desc-count')).toHaveText('10 / 20');
    await page.locator('#desc').pressSequentially('0123456789ABC');
    await expect(page.locator('#desc-count')).toHaveText('20 / 20');
  });
});
