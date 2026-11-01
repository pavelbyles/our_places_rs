import { test as base, expect } from '@playwright/test';
import * as path from 'node:path';

const CSP_LISTENER = path.resolve(__dirname, 'csp-listener.js');

/**
 * Base test for all portal specs. It fails any test during which the
 * browser reports a Content-Security-Policy
 * violation (spec acceptance criterion: zero CSP violations).
 */
export const test = base.extend<{ cspGuard: void }>({
  cspGuard: [
    async ({ page }, use) => {
      const violations: string[] = [];
      await page.exposeFunction('__reportCspViolation', (v: string) => violations.push(v));
      page.on('console', (msg) => {
        if (/Content[- ]Security[- ]Policy|Refused to (execute|load|apply|evaluate)/i.test(msg.text())) {
          violations.push(`console: ${msg.text()}`);
        }
      });
      await page.addInitScript({ path: CSP_LISTENER });
      await use();
      expect(violations, 'Content-Security-Policy violations').toEqual([]);
    },
    { auto: true },
  ],
});

export { expect };
