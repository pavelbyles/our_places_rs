import { randomInt } from 'node:crypto';
import type { Page } from '@playwright/test';
import { test, expect } from '../fixtures/base.fixture.js';
import { createHost, loginAdminAs } from '../fixtures/admin-data.js';
import { createListingCopy, psql } from '../fixtures/bookings-data.js';

function createHostWithListing() {
  const host = createHost();
  const listing = createListingCopy();
  psql(`
    UPDATE listing SET user_id = '${host.id}' WHERE id = '${listing.id}';
    INSERT INTO host_profiles (user_id, verified_status)
    VALUES ('${host.id}', 'verified')
    ON CONFLICT (user_id) DO UPDATE SET verified_status = 'verified';
  `);
  return { host, listing };
}

function toPathAndSearch(rawUrl: string): string {
  const parsed = new URL(rawUrl);
  return `${parsed.pathname}${parsed.search}`;
}

async function signInAdminPortal(page: Page, email: string, password: string): Promise<void> {
  await page.goto('/login');
  await page.locator('#admin-login-email').fill(email);
  await page.locator('#admin-login-password').fill(password);
  await page.locator('#btn-admin-login').click();
  await page.waitForFunction(() => !window.location.pathname.includes('/login'), {
    timeout: 15_000,
  });
}

test.describe('Admin Portal - Host Staff Roster & Onboarding Flows', () => {
  test(
    'host invites shared multi-host cleaner, cleaner sets password via invite and uses forgot-password',
    { tag: '@new-only' },
    async ({ browser }) => {
      test.setTimeout(60_000);
      const tag = `${Date.now()}-${randomInt(10_000, 99_999)}`;
      const cleanerEmail = `e2e.cleaner.${tag}@example.com`;
      const initialPassword = `CleanPass!${randomInt(1000, 9999)}`;
      const resetPassword = `CleanReset!${randomInt(1000, 9999)}`;

      const { host: hostA, listing: listingA } = createHostWithListing();
      const { host: hostB, listing: listingB } = createHostWithListing();

      const ctxA = await browser.newContext({ baseURL: 'http://localhost:3012' });
      const ctxB = await browser.newContext({ baseURL: 'http://localhost:3012' });
      const ctxStaff = await browser.newContext({ baseURL: 'http://localhost:3012' });

      try {
        const pageA = await ctxA.newPage();
        await loginAdminAs(pageA, hostA);
        await pageA.goto('/admin/staff-users');

        // Host A sees only Host A's listing checkbox in the invite form.
        const inviteFormA = pageA.locator('#staff-invite-form');
        await expect(inviteFormA).toContainText(listingA.name);
        await expect(inviteFormA).not.toContainText(listingB.name);

        await pageA.locator('#staff-invite-email').fill(cleanerEmail);
        await pageA.locator('#staff-invite-first-name').fill('Dana');
        await pageA.locator('#staff-invite-last-name').fill('Clean');
        await inviteFormA.locator(`input[type="checkbox"][name="listing_${listingA.id}"]`).check();
        await pageA.locator('#btn-staff-invite-submit').click();

        await expect(pageA.locator('#staff-users-notice')).toContainText('Staff member saved');
        const staffId = psql(`SELECT id FROM "user" WHERE lower(email) = lower('${cleanerEmail}');`);
        expect(staffId).toMatch(/^[0-9a-f-]{36}$/);

        const cardA = pageA.locator(`#staff-card-${staffId}`);
        await expect(cardA).toContainText('Dana Clean');
        await expect(cardA).toContainText(cleanerEmail);
        await expect(pageA.locator(`#staff-status-${staffId}`)).toHaveText('Pending password setup');

        let setupUrlA = '';
        await expect
          .poll(
            () => {
              setupUrlA = psql(`
                SELECT payload->>'setup_url'
                FROM email_outbox
                WHERE lower(recipient_email) = lower('${cleanerEmail}')
                  AND template_id = 'staff_invite'
                ORDER BY created_at DESC
                LIMIT 1;
              `);
              return setupUrlA;
            },
            { timeout: 10_000 },
          )
          .toContain('/reset-password?token=');

        // Host B invites the SAME cleaner email for Listing B. Profile name is preserved & hosts stay isolated.
        const pageB = await ctxB.newPage();
        await loginAdminAs(pageB, hostB);
        await pageB.goto('/admin/staff-users');

        const inviteFormB = pageB.locator('#staff-invite-form');
        await expect(inviteFormB).toContainText(listingB.name);
        await expect(inviteFormB).not.toContainText(listingA.name);

        await pageB.locator('#staff-invite-email').fill(cleanerEmail);
        await pageB.locator('#staff-invite-first-name').fill('OverwriteAttempt');
        await pageB.locator('#staff-invite-last-name').fill('Ignored');
        await inviteFormB.locator(`input[type="checkbox"][name="listing_${listingB.id}"]`).check();
        await pageB.locator('#btn-staff-invite-submit').click();

        await expect(pageB.locator('#staff-users-notice')).toContainText('Staff member saved');
        const cardB = pageB.locator(`#staff-card-${staffId}`);
        await expect(cardB).toContainText('Dana Clean');
        await expect(cardB).not.toContainText('OverwriteAttempt');
        await expect(cardB).toContainText(listingB.name);
        await expect(cardB).not.toContainText(listingA.name);

        expect(psql(`SELECT COUNT(*) FROM host_staff WHERE staff_user_id = '${staffId}';`)).toBe('2');

        // Re-inviting a password-pending cleaner rotates the single-use invite token cleanly.
        let setupUrlB = '';
        await expect
          .poll(
            () => {
              setupUrlB = psql(`
                SELECT payload->>'setup_url'
                FROM email_outbox
                WHERE lower(recipient_email) = lower('${cleanerEmail}')
                  AND template_id = 'staff_invite'
                ORDER BY created_at DESC
                LIMIT 1;
              `);
              return setupUrlB !== setupUrlA ? setupUrlB : '';
            },
            { timeout: 10_000 },
          )
          .toContain('/reset-password?token=');

        // Reload Host A's page: Host A still cannot see Listing B under the shared cleaner.
        await pageA.reload();
        await expect(pageA.locator(`#staff-card-${staffId}`)).toContainText(listingA.name);
        await expect(pageA.locator(`#staff-card-${staffId}`)).not.toContainText(listingB.name);

        // Superseded invite link (Token A) is rejected; latest setup link (Token B) sets initial password.
        const pageStaff = await ctxStaff.newPage();
        await pageStaff.goto(toPathAndSearch(setupUrlA));
        await pageStaff.locator('#reset-new-password').fill(initialPassword);
        await pageStaff.locator('#reset-confirm-password').fill(initialPassword);
        await pageStaff.locator('#btn-reset-password-submit').click();
        await expect(pageStaff.locator('#reset-password-expired')).toBeVisible();

        await pageStaff.goto(toPathAndSearch(setupUrlB));
        await expect(pageStaff.locator('#reset-new-password')).toBeVisible();

        await pageStaff.locator('#reset-new-password').fill(initialPassword);
        await pageStaff.locator('#reset-confirm-password').fill(initialPassword);
        await pageStaff.locator('#btn-reset-password-submit').click();

        await expect(pageStaff).toHaveURL(/\/login(\?|$)/);
        await expect(pageStaff.locator('#admin-auth-notice')).toBeVisible();

        // Single-use invite link is now exhausted on submit (GET avoids token oracle).
        await pageStaff.goto(toPathAndSearch(setupUrlB));
        await pageStaff.locator('#reset-new-password').fill(initialPassword);
        await pageStaff.locator('#reset-confirm-password').fill(initialPassword);
        await pageStaff.locator('#btn-reset-password-submit').click();
        await expect(pageStaff.locator('#reset-password-expired')).toBeVisible();

        // Cleaner signs in and lands directly on Turnover Dispatch; roster flips to Active.
        await signInAdminPortal(pageStaff, cleanerEmail, initialPassword);
        await expect(pageStaff).toHaveURL(/\/admin\/turnovers(\?|$)/);

        await pageA.reload();
        await expect(pageA.locator(`#staff-status-${staffId}`)).toHaveText('Active');

        // Self-service forgot-password flow also works end-to-end for cleaning staff.
        const ctxReset = await browser.newContext({ baseURL: 'http://localhost:3012' });
        try {
          const pageReset = await ctxReset.newPage();
          await pageReset.goto('/login');
          await pageReset.locator('#admin-forgot-password-link').click();
          await expect(pageReset).toHaveURL(/\/forgot-password(\?|$)/);

          await pageReset.locator('#forgot-password-email').fill(cleanerEmail);
          await pageReset.locator('#btn-forgot-password-submit').click();
          await expect(pageReset.locator('#forgot-password-sent')).toBeVisible();

          let resetUrl = '';
          await expect
            .poll(
              () => {
                resetUrl = psql(`
                  SELECT payload->>'reset_url'
                  FROM email_outbox
                  WHERE lower(recipient_email) = lower('${cleanerEmail}')
                    AND template_id = 'password_reset_link'
                  ORDER BY created_at DESC
                  LIMIT 1;
                `);
                return resetUrl;
              },
              { timeout: 10_000 },
            )
            .toContain('/reset-password?token=');

          await pageReset.goto(toPathAndSearch(resetUrl));
          await pageReset.locator('#reset-new-password').fill(resetPassword);
          await pageReset.locator('#reset-confirm-password').fill(resetPassword);
          await pageReset.locator('#btn-reset-password-submit').click();

          await expect(pageReset).toHaveURL(/\/login(\?|$)/);
          await expect(pageReset.locator('#admin-auth-notice')).toBeVisible();

          // Old password fails; new password signs cleanly into /admin/turnovers.
          await pageReset.locator('#admin-login-email').fill(cleanerEmail);
          await pageReset.locator('#admin-login-password').fill(initialPassword);
          await pageReset.locator('#btn-admin-login').click();
          await expect(pageReset.locator('#admin-auth-error')).toBeVisible();

          await signInAdminPortal(pageReset, cleanerEmail, resetPassword);
          await expect(pageReset).toHaveURL(/\/admin\/turnovers(\?|$)/);
        } finally {
          await ctxReset.close();
        }
      } finally {
        await Promise.all([ctxA.close(), ctxB.close(), ctxStaff.close()]);
      }
    },
  );

  test(
    'restricts staff accounts exclusively to /admin/turnovers and returns 403 on all other admin routes',
    { tag: '@new-only' },
    async ({ browser }) => {
      test.setTimeout(60_000);
      const tag = `${Date.now()}-${randomInt(10_000, 99_999)}`;
      const cleanerEmail = `e2e.isolated.staff.${tag}@example.com`;
      const staffPassword = `StaffPass!${randomInt(1000, 9999)}`;

      const { host, listing } = createHostWithListing();

      const ctxHost = await browser.newContext({ baseURL: 'http://localhost:3012' });
      const ctxStaff = await browser.newContext({ baseURL: 'http://localhost:3012' });

      try {
        const pageHost = await ctxHost.newPage();
        await loginAdminAs(pageHost, host);
        await pageHost.goto('/admin/staff-users');
        await pageHost.locator('#staff-invite-email').fill(cleanerEmail);
        await pageHost.locator('#staff-invite-first-name').fill('Sam');
        await pageHost.locator('#staff-invite-last-name').fill('Porter');
        await pageHost
          .locator(`#staff-invite-form input[type="checkbox"][name="listing_${listing.id}"]`)
          .check();
        await pageHost.locator('#btn-staff-invite-submit').click();
        await expect(pageHost.locator('#staff-users-notice')).toBeVisible();

        let setupUrl = '';
        await expect
          .poll(
            () => {
              setupUrl = psql(`
                SELECT payload->>'setup_url'
                FROM email_outbox
                WHERE lower(recipient_email) = lower('${cleanerEmail}')
                  AND template_id = 'staff_invite'
                ORDER BY created_at DESC
                LIMIT 1;
              `);
              return setupUrl;
            },
            { timeout: 10_000 },
          )
          .toContain('/reset-password?token=');

        const pageStaff = await ctxStaff.newPage();
        await pageStaff.goto(toPathAndSearch(setupUrl));
        await pageStaff.locator('#reset-new-password').fill(staffPassword);
        await pageStaff.locator('#reset-confirm-password').fill(staffPassword);
        await pageStaff.locator('#btn-reset-password-submit').click();

        await signInAdminPortal(pageStaff, cleanerEmail, staffPassword);
        await expect(pageStaff).toHaveURL(/\/admin\/turnovers(\?|$)/);

        // Root index redirects Staff directly to Turnover Dispatch.
        await pageStaff.goto('/');
        await expect(pageStaff).toHaveURL(/\/admin\/turnovers(\?|$)/);

        // Desktop sidebar exposes only Turnover Dispatch (hides bookings, listings, payouts, users, staff-users).
        await expect(pageStaff.locator('#admin-sidebar-turnovers-link')).toBeVisible();
        await expect(pageStaff.locator('#admin-sidebar-staff-users-link')).toHaveCount(0);
        await expect(pageStaff.locator('aside a[href="/admin/bookings"]')).toHaveCount(0);
        await expect(pageStaff.locator('aside a[href="/admin/listings"]')).toHaveCount(0);
        await expect(pageStaff.locator('aside a[href="/admin/payouts"]')).toHaveCount(0);
        await expect(pageStaff.locator('aside a[href="/admin/users"]')).toHaveCount(0);

        for (const forbiddenPath of [
          '/admin',
          '/admin/bookings',
          '/admin/listings',
          '/admin/payouts',
          '/admin/users',
          '/admin/staff-users',
        ]) {
          const res = await pageStaff.goto(forbiddenPath);
          expect(res?.status(), `expected HTTP 403 for ${forbiddenPath}`).toBe(403);
        }
      } finally {
        await Promise.all([ctxHost.close(), ctxStaff.close()]);
      }
    },
  );
});
