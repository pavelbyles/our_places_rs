import { test, expect } from '../fixtures/auth.fixture.js';

test.describe('Admin Portal - Executive Dashboard', () => {
  test.beforeEach(async ({ authenticatedAdminPage }) => {
    await authenticatedAdminPage.goto('/admin');
  });

  test('should render executive dashboard with title and quick actions', async ({ authenticatedAdminPage }) => {
    // Check main title
    const heading = authenticatedAdminPage.locator('h1');
    await expect(heading).toContainText('Executive Dashboard');

    // Check quick action links
    const newVillaBtn = authenticatedAdminPage.locator('main a[href="/admin/listings/new"], a.btn:has-text("New Villa")').first();
    await expect(newVillaBtn).toBeVisible();

    const viewBookingsBtn = authenticatedAdminPage.locator('main a[href="/admin/bookings"], a.btn:has-text("Schedule")').first();
    await expect(viewBookingsBtn).toBeVisible();
  });

  test('should display key performance indicator (KPI) metric cards', async ({ authenticatedAdminPage }) => {
    // Verify KPI grid cards
    await expect(authenticatedAdminPage.locator('text=Active Villas')).toBeVisible();
    await expect(authenticatedAdminPage.locator('text=Active Holds')).toBeVisible();
    await expect(authenticatedAdminPage.locator('text=Bookings Revenue')).toBeVisible();
  });
});
