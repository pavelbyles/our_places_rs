import type { TestInfo } from '@playwright/test';

/**
 * True when the test runs against the redesigned portals (projects named
 * `*-portal-new-*`). Use it only for behavior the parity inventory records as
 * a deliberate difference, such as legacy browser-storage auth.
 */
export function isNewPortal(testInfo: TestInfo): boolean {
  return testInfo.project.name.includes('-new-');
}
