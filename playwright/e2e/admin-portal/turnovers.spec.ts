import { randomInt } from 'node:crypto';
import type { APIRequestContext, Page } from '@playwright/test';
import { test, expect } from '../fixtures/base.fixture.js';
import { createHost, loginAdminAs } from '../fixtures/admin-data.js';
import {
  createBooking,
  createGuest,
  createListingCopy,
  psql,
  signTestJwt,
} from '../fixtures/bookings-data.js';

const BOOKING_API_URL = process.env.BOOKING_API_URL || 'http://127.0.0.1:8081';

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

async function inviteAndActivateStaff(
  hostPage: Page,
  staffPage: Page,
  listingId: string,
  email: string,
  firstName: string,
  lastName: string,
  password: string,
): Promise<void> {
  await hostPage.goto('/admin/staff-users');
  await hostPage.locator('#staff-invite-email').fill(email);
  await hostPage.locator('#staff-invite-first-name').fill(firstName);
  await hostPage.locator('#staff-invite-last-name').fill(lastName);
  await hostPage
    .locator(`#staff-invite-form input[type="checkbox"][name="listing_${listingId}"]`)
    .check();
  await hostPage.locator('#btn-staff-invite-submit').click();
  await expect(hostPage.locator('#staff-users-notice')).toBeVisible();

  let setupUrl = '';
  await expect
    .poll(
      () => {
        setupUrl = psql(`
          SELECT payload->>'setup_url'
          FROM email_outbox
          WHERE lower(recipient_email) = lower('${email}')
            AND template_id = 'staff_invite'
          ORDER BY created_at DESC
          LIMIT 1;
        `);
        return setupUrl;
      },
      { timeout: 10_000 },
    )
    .toContain('/reset-password?token=');

  await staffPage.goto(toPathAndSearch(setupUrl));
  await staffPage.locator('#reset-new-password').fill(password);
  await staffPage.locator('#reset-confirm-password').fill(password);
  await staffPage.locator('#btn-reset-password-submit').click();
  await expect(staffPage).toHaveURL(/\/login(\?|$)/);

  await signInAdminPortal(staffPage, email, password);
  await expect(staffPage).toHaveURL(/\/admin\/turnovers(\?|$)/);
}

async function confirmBookingViaApi(
  request: APIRequestContext,
  bookingId: string,
  hostId: string,
): Promise<void> {
  const res = await request.patch(`${BOOKING_API_URL}/api/v1/bookings/${bookingId}`, {
    headers: { Authorization: `Bearer ${signTestJwt(hostId)}` },
    data: { status: 'confirmed' },
  });
  expect(res.status(), 'confirming booking via booking_api').toBe(200);
}

test.describe('Admin Portal - Mobile Turnover Dispatch, Transitions & Comment Threads', () => {
  test(
    'renders mobile dispatch board, enforces role transitions & unsetting Inspected & Ready, creates manual turnover, and notifies host on comments',
    { tag: '@new-only' },
    async ({ browser, request }) => {
      test.setTimeout(60_000);
      const tag = `${Date.now()}-${randomInt(10_000, 99_999)}`;
      const cleanerEmail = `e2e.dispatch.cleaner.${tag}@example.com`;
      const otherCleanerEmail = `e2e.unassigned.cleaner.${tag}@example.com`;
      const cleanerPassword = `TurnPass!${randomInt(1000, 9999)}`;

      const { host, listing } = createHostWithListing();
      const { host: otherHost, listing: otherListing } = createHostWithListing();

      const guest = createGuest();
      const booking = await createBooking(request, guest.id, {
        confirmed: false,
        listingId: listing.id,
      });
      await confirmBookingViaApi(request, booking.id, host.id);

      let bookingTurnoverId = '';
      await expect
        .poll(
          () => {
            bookingTurnoverId = psql(`
              SELECT id::text
              FROM turnover
              WHERE booking_id = '${booking.id}'
                AND source_kind = 'booking'
              LIMIT 1;
            `);
            return bookingTurnoverId;
          },
          { timeout: 10_000 },
        )
        .toMatch(/^[0-9a-f-]{36}$/);

      const ctxHost = await browser.newContext({ baseURL: 'http://localhost:3012' });
      const ctxStaffMobile = await browser.newContext({
        baseURL: 'http://localhost:3012',
        viewport: { width: 390, height: 844 },
      });
      const ctxOtherHost = await browser.newContext({ baseURL: 'http://localhost:3012' });
      const ctxOtherStaff = await browser.newContext({ baseURL: 'http://localhost:3012' });

      try {
        const pageHost = await ctxHost.newPage();
        await loginAdminAs(pageHost, host);

        const pageStaff = await ctxStaffMobile.newPage();
        await inviteAndActivateStaff(
          pageHost,
          pageStaff,
          listing.id,
          cleanerEmail,
          'Meryl',
          'Wash',
          cleanerPassword,
        );

        // Mobile viewport layout check: no horizontal overflow and window tabs meet 44px touch targets.
        const boardOverflow = await pageStaff.evaluate(
          () => document.documentElement.scrollWidth <= window.innerWidth,
        );
        expect(boardOverflow, 'mobile board must not horizontally overflow').toBe(true);
        const upcomingTab = pageStaff.locator('[data-window-tab="upcoming"]');
        const tabBox = await upcomingTab.boundingBox();
        expect(tabBox?.height ?? 0).toBeGreaterThanOrEqual(44);

        // Booking created in future appears in Upcoming tab.
        await upcomingTab.click();
        await expect(pageStaff).toHaveURL(/\/admin\/turnovers\?.*window=upcoming/);

        const bookingCard = pageStaff.locator(`#turnover-card-${bookingTurnoverId}`);
        await expect(bookingCard).toBeVisible();
        await expect(bookingCard).toHaveAttribute('data-turnover-status', 'needs_cleaning');

        // Touch targets on card transitions must be >= 44px and page stays overflow-free.
        const completeBtn = bookingCard.locator('[data-status-target="cleaning_completed"]');
        const completeBox = await completeBtn.boundingBox();
        expect(completeBox?.height ?? 0).toBeGreaterThanOrEqual(44);
        expect(
          await pageStaff.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth),
        ).toBe(true);

        // Zero guest PII or financial figures leaked anywhere on the dispatch board.
        const pageBodyText = await pageStaff.locator('main').innerText();
        expect(pageBodyText).not.toContain(guest.email);
        expect(pageBodyText).not.toContain('Casey');
        expect(pageBodyText).not.toContain(booking.confirmationCode);
        expect(pageBodyText).not.toMatch(/\/admin\/bookings|payout|subtotal/i);

        // Staff cannot transition directly to Inspected & Ready or cancel booking turnover as Not required.
        await expect(bookingCard.locator('[data-status-target="inspected_and_ready"]')).toHaveCount(0);
        await expect(bookingCard.locator('[data-status-target="not_required"]')).toHaveCount(0);

        // Staff marks Cleaning Completed via HTMX without losing scroll/page context.
        await completeBtn.click();
        await expect(bookingCard).toHaveAttribute('data-turnover-status', 'cleaning_completed');

        // Host marks Cleaning Completed -> Inspected & Ready on the board.
        await pageHost.goto('/admin/turnovers?window=upcoming');
        const hostBookingCard = pageHost.locator(`#turnover-card-${bookingTurnoverId}`);
        await expect(hostBookingCard).toHaveAttribute('data-turnover-status', 'cleaning_completed');
        await hostBookingCard.locator('[data-status-target="inspected_and_ready"]').click();
        await expect(hostBookingCard).toHaveAttribute('data-turnover-status', 'inspected_and_ready');

        // Staff can unset Inspected & Ready when maintenance is required.
        await pageStaff.reload();
        await expect(bookingCard).toHaveAttribute('data-turnover-status', 'inspected_and_ready');
        await bookingCard.locator('[data-status-target="maintenance_required"]').click();
        await expect(bookingCard).toHaveAttribute('data-turnover-status', 'maintenance_required');

        // Staff creates a manual turnover on Today tab and posts a threaded HTMX comment.
        await pageStaff.locator('[data-window-tab="today"]').click();
        const manualSummary = pageStaff.locator('details:has(#manual-turnover-form) > summary');
        await manualSummary.click();

        const manualTitle = `AC Deep Clean ${tag}`;
        await pageStaff.locator('#manual-listing-id').selectOption(listing.id);
        await pageStaff.locator('#manual-title').fill(manualTitle);
        await pageStaff.locator('#btn-create-manual-turnover').click();

        let manualTurnoverId = '';
        await expect
          .poll(
            () => {
              manualTurnoverId = psql(`
                SELECT id::text
                FROM turnover
                WHERE listing_id = '${listing.id}'
                  AND source_kind = 'manual'
                  AND title = '${manualTitle}'
                LIMIT 1;
              `);
              return manualTurnoverId;
            },
            { timeout: 10_000 },
          )
          .toMatch(/^[0-9a-f-]{36}$/);

        const manualCard = pageStaff.locator(`#turnover-card-${manualTurnoverId}`);
        await expect(manualCard).toBeVisible();
        await expect(manualCard).toContainText(manualTitle);

        // Navigate into turnover detail page and post HTMX comment.
        await manualCard
          .locator(`a[href="/admin/turnovers/${manualTurnoverId}"]`)
          .filter({ hasText: 'Notes & Details' })
          .click();
        await expect(pageStaff).toHaveURL(new RegExp(`/admin/turnovers/${manualTurnoverId}$`));

        const staffNote = `Replaced AC filter and spotted bathroom tile grout crack (${tag})`;
        await pageStaff.locator('#turnover-comment-body').fill(staffNote);
        await pageStaff.locator('#btn-post-turnover-comment').click();

        const commentsList = pageStaff.locator('#turnover-comments-list');
        await expect(commentsList).toContainText(staffNote);
        await expect(pageStaff.locator('#turnover-comment-body')).toHaveValue('');

        // Outbox email queued for host (and never to the comment author).
        await expect
          .poll(
            () =>
              psql(`
                SELECT COUNT(*)
                FROM email_outbox
                WHERE template_id = 'turnover_comment_notification'
                  AND lower(recipient_email) = lower('${host.email}')
                  AND payload->>'comment_body' = '${staffNote}';
              `),
            { timeout: 10_000 },
          )
          .toBe('1');
        expect(
          psql(`
            SELECT COUNT(*)
            FROM email_outbox
            WHERE template_id = 'turnover_comment_notification'
              AND lower(recipient_email) = lower('${cleanerEmail}')
              AND payload->>'comment_body' = '${staffNote}';
          `),
        ).toBe('0');

        // Verify host sees comment + reply triggers notification back to assigned staff.
        await pageHost.goto(`/admin/turnovers/${manualTurnoverId}`);
        await expect(pageHost.locator('#turnover-comments-list')).toContainText(staffNote);
        const hostReply = `Grout tech booked for 3pm tomorrow (${tag})`;
        await pageHost.locator('#turnover-comment-body').fill(hostReply);
        await pageHost.locator('#btn-post-turnover-comment').click();
        await expect(pageHost.locator('#turnover-comments-list')).toContainText(hostReply);

        await expect
          .poll(
            () =>
              psql(`
                SELECT COUNT(*)
                FROM email_outbox
                WHERE template_id = 'turnover_comment_notification'
                  AND lower(recipient_email) = lower('${cleanerEmail}')
                  AND payload->>'comment_body' = '${hostReply}';
              `),
            { timeout: 10_000 },
          )
          .toBe('1');

        // Unassigned cleaner for another host/listing cannot view either turnover on board or detail route.
        const pageOtherHost = await ctxOtherHost.newPage();
        await loginAdminAs(pageOtherHost, otherHost);
        const pageOtherStaff = await ctxOtherStaff.newPage();
        await inviteAndActivateStaff(
          pageOtherHost,
          pageOtherStaff,
          otherListing.id,
          otherCleanerEmail,
          'Other',
          'Crew',
          cleanerPassword,
        );
        await pageOtherStaff.goto('/admin/turnovers?window=all');
        await expect(pageOtherStaff.locator(`#turnover-card-${bookingTurnoverId}`)).toHaveCount(0);
        await expect(pageOtherStaff.locator(`#turnover-card-${manualTurnoverId}`)).toHaveCount(0);

        const isolatedResp = await pageOtherStaff.goto(`/admin/turnovers/${manualTurnoverId}`);
        expect([403, 404]).toContain(isolatedResp?.status() ?? 0);
      } finally {
        await Promise.all([
          ctxHost.close(),
          ctxStaffMobile.close(),
          ctxOtherHost.close(),
          ctxOtherStaff.close(),
        ]);
      }
    },
  );
});
