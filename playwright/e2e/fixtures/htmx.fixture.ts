import { test as base, type Page, type Locator } from '@playwright/test';

/**
 * Waits for active HTMX network requests and DOM settlement to complete on the given page.
 */
export async function waitForHtmx(page: Page, timeoutMs = 5000): Promise<void> {
  // Wait until no elements possess the 'htmx-request' class
  await page.waitForFunction(
    () => document.querySelectorAll('.htmx-request').length === 0,
    { timeout: timeoutMs }
  );
}

/**
 * Custom test fixture extending Playwright base test with Topcoat/HTMX helpers.
 */
export const test = base.extend<{
  waitForHtmx: (timeoutMs?: number) => Promise<void>;
}>({
  waitForHtmx: async ({ page }, use) => {
    await use((timeoutMs?: number) => waitForHtmx(page, timeoutMs));
  },
});

export { expect } from '@playwright/test';
