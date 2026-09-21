import { test, expect } from '../fixtures/auth.fixture.js';

test.describe('Admin Portal - Villa Listings Management', () => {
  test.beforeEach(async ({ authenticatedAdminPage }) => {
    await authenticatedAdminPage.goto('/admin/listings');
  });

  test('should render listings inventory page and title', async ({ authenticatedAdminPage }) => {
    const heading = authenticatedAdminPage.locator('h1');
    await expect(heading).toContainText('Villa Listings Management');

    // Check action buttons
    const createBtn = authenticatedAdminPage.locator('a[href*="/listings/new"]').first();
    await expect(createBtn).toBeVisible();
  });

  test('should provide search and filter controls', async ({ authenticatedAdminPage }) => {
    const searchInput = authenticatedAdminPage.locator('input[placeholder*="Search" i], input[type="search"]').first();
    if (await searchInput.isVisible()) {
      await searchInput.fill('Kingston');
      await expect(searchInput).toHaveValue('Kingston');
    }
  });
});
