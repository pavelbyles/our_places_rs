import type { Page } from '@playwright/test';
import { test, expect } from '../fixtures/auth.fixture.js';
import { createHost, loginAdminAs } from '../fixtures/admin-data.js';

/**
 * Admin-managed currencies and exchange rates (#100).
 *
 * Each browser project works on its own scratch currency and USD pair so the
 * projects never race on shared rows, and every step tolerates data left by
 * earlier runs (currencies are never deleted; the audit log is append-only).
 */
test.describe.configure({ mode: 'serial' });

function scratchCode(): string {
  return test.info().project.name.includes('firefox') ? 'TTD' : 'XCD';
}

async function ensureCurrency(page: Page, code: string): Promise<void> {
  await page.goto('/admin/exchange-rates');
  const row = page.locator(`#currencies-table tr[data-currency="${code}"]`);
  if ((await row.count()) === 0) {
    const form = page.locator('#add-currency-form');
    await form.locator('input[name="code"]').fill(code);
    await form.locator('input[name="name"]').fill(`${code} e2e dollar`);
    await form.getByRole('button', { name: 'Add currency' }).click();
    await expect(row).toBeVisible();
  }
  // exact: "Activate" must not match "Deactivate".
  const activate = row.getByRole('button', { name: 'Activate', exact: true });
  if ((await activate.count()) > 0) {
    await activate.click();
    await expect(row.getByRole('button', { name: 'Deactivate', exact: true })).toBeVisible();
  }
}

async function publish(page: Page, base: string, target: string, rate: string, effectiveAt = ''): Promise<void> {
  const form = page.locator('#publish-rate-form');
  await form.locator('select[name="base"]').selectOption(base);
  await form.locator('select[name="target"]').selectOption(target);
  await form.locator('input[name="rate"]').fill(rate);
  await form.locator('input[name="effective_at"]').fill(effectiveAt);
  await form.getByRole('button', { name: /Publish rate|Confirm and publish/ }).click();
}

function pairRow(page: Page, base: string, target: string) {
  return page.locator('#exchange-rates-table tr', { hasText: `${base} / ${target}` });
}

test.describe('Admin Portal - Currencies & rates', { tag: '@new-only' }, () => {
  test('adds a currency and publishes its first rate', async ({ authenticatedAdminPage: page }) => {
    const code = scratchCode();
    await ensureCurrency(page, code);
    await page.goto('/admin/exchange-rates');
    if ((await pairRow(page, 'USD', code).count()) === 0) {
      await publish(page, 'USD', code, '2.70');
      await page.waitForURL(/notice=published/);
    }
    await expect(pairRow(page, 'USD', code)).toContainText(/\d+\.\d{6}/);
  });

  test('a change over 5% needs confirmation', async ({ authenticatedAdminPage: page }) => {
    const code = scratchCode();
    await page.goto('/admin/exchange-rates');
    const current = Number(
      ((await pairRow(page, 'USD', code).locator('td').nth(1).textContent()) ?? '0').trim(),
    );
    expect(current).toBeGreaterThan(0);
    const proposed = (current * 1.1).toFixed(2);

    await publish(page, 'USD', code, proposed);
    const confirm = page.locator('#rate-confirm-alert');
    await expect(confirm).toBeVisible();
    await expect(confirm).toContainText('%');
    await page.locator('#publish-rate-form').getByRole('button', { name: 'Confirm and publish' }).click();
    await page.waitForURL(/notice=published/);
    await expect(pairRow(page, 'USD', code)).toContainText(Number(proposed).toFixed(6));
  });

  test('a scheduled rate shows in history and can be deleted', async ({ authenticatedAdminPage: page }) => {
    const code = scratchCode();
    await page.goto('/admin/exchange-rates');
    const current = ((await pairRow(page, 'USD', code).locator('td').nth(1).textContent()) ?? '').trim();
    const tomorrow = new Date(Date.now() + 24 * 3600 * 1000);
    tomorrow.setUTCSeconds(0, 0);
    const local = tomorrow.toISOString().slice(0, 16);

    await publish(page, 'USD', code, Number(current).toFixed(2), local);
    await page.waitForURL(/notice=published/);
    // Scheduled rates do not change the current table.
    await expect(pairRow(page, 'USD', code)).toContainText(current);

    await page.goto(`/admin/exchange-rates/USD/${code}`);
    const scheduled = page.locator('#pair-history-table tr', { hasText: 'Scheduled' });
    await expect(scheduled).toHaveCount(1);
    page.once('dialog', (d) => d.accept());
    await scheduled.getByRole('button', { name: 'Delete' }).click();
    await expect(page.locator('#pair-history-table tr', { hasText: 'Scheduled' })).toHaveCount(0);
  });

  test('retiring a pair used by listings is blocked and lists them', async ({ authenticatedAdminPage: page }) => {
    const code = scratchCode();
    await page.goto(`/admin/exchange-rates/USD/${code}`);
    page.once('dialog', (d) => d.accept());
    await page.getByRole('button', { name: 'Retire pair' }).click();
    const blocked = page.locator('#currency-blocked-dialog');
    await expect(blocked).toBeVisible();
    await expect(blocked.getByRole('link').first()).toHaveAttribute('href', /\/admin\/listings\/[0-9a-f-]+\/edit/);
  });

  test('guests can choose the new currency', async ({ page }) => {
    const code = scratchCode();
    // Poll: a portal reused from a local dev session may still cache the list.
    await expect
      .poll(
        async () => {
          await page.goto('http://localhost:3010/');
          return page.locator(`a[href="?currency=${code}"]`).count();
        },
        { timeout: 90_000, intervals: [1_000, 5_000, 10_000] },
      )
      .toBeGreaterThan(0);
  });

  test('hosts cannot publish rates', async ({ page }) => {
    await loginAdminAs(page, createHost());
    await page.goto('/admin/exchange-rates');
    await expect(page.locator('#publish-rate-form')).toHaveCount(0);
    const resp = await page.request.post('/admin/exchange-rates', {
      form: { base: 'USD', target: 'JMD', rate: '161' },
      maxRedirects: 0,
    });
    expect(resp.status()).toBe(403);
  });
});
