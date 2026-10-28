import { test, expect } from '../fixtures/base.fixture.js';
import { createBooking, createGuest, loginGuest } from '../fixtures/bookings-data.js';

// Spec AC 6-8: server-side auth, ownership checks and strict CSP on the
// redesigned guest portal.
const NEW_ONLY = { tag: '@new-only' };
const PUBLIC_PAGES = ['/', '/listings', '/about', '/login', '/register'];

test.describe('Guest Portal - Security', () => {
  test('never sends signed-in navigation to a signed-out visitor', NEW_ONLY, async ({ request }) => {
    for (const path of PUBLIC_PAGES) {
      const res = await request.get(path);
      expect(res.status(), path).toBe(200);
      const html = await res.text();
      expect(html, path).not.toContain('nav-user-dropdown');
      expect(html, path).not.toContain('action="/logout"');
    }
  });

  test('sends a strict CSP and hardening headers on every page', NEW_ONLY, async ({ request }) => {
    for (const path of [...PUBLIC_PAGES, '/no-such-page']) {
      const h = (await request.get(path)).headers();
      expect(h['content-security-policy'], path).toContain("script-src 'self';");
      expect(h['content-security-policy'], path).not.toContain('unsafe-');
      expect(h['content-security-policy'], path).toContain("frame-ancestors 'none'");
      expect(h['x-content-type-options'], path).toBe('nosniff');
    }
  });

  test("returns 404 for every route of another guest's booking", NEW_ONLY, async ({ page, request }) => {
    const other = await createBooking(request, createGuest().id, { confirmed: true });
    await loginGuest(page, createGuest());
    for (const path of [
      `/bookings/${other.id}/change-dates`,
      `/bookings/${other.id}/cancellation-quote`,
      `/bookings/${other.id}/cancel`,
      `/bookings/${other.id}/documents/REC-2026-${other.confirmationCode}`,
      `/bookings/${other.confirmationCode}/messages`,
    ]) {
      const res = await page.goto(path);
      expect(res?.status(), path).toBe(404);
    }
    const origin = new URL(page.url()).origin;
    const post = await page.request.post(`/bookings/${other.id}/cancel`, { headers: { Origin: origin } });
    expect(post.status()).toBe(404);
  });

  test('rejects a cross-site form post', NEW_ONLY, async ({ page }) => {
    await loginGuest(page, createGuest());
    const res = await page.request.post('/logout', {
      headers: { Origin: 'https://evil.example', 'Sec-Fetch-Site': 'cross-site' },
    });
    expect(res.status()).toBe(403);
  });

  test('never calls backend services from the browser', NEW_ONLY, async ({ page }) => {
    const offsite: string[] = [];
    page.on('request', (req) => {
      const url = new URL(req.url());
      const backend = /:(808[0-9])$/.test(url.host);
      const scripted = ['fetch', 'xhr'].includes(req.resourceType()) && url.host !== 'localhost:3010';
      // Listing photos load from Cloud Storage by design; anything else is a leak.
      if (backend || scripted) offsite.push(req.url());
    });
    const guest = createGuest();
    await loginGuest(page, guest);
    await page.goto('/listings');
    await page.goto('/bookings');
    await page.goto('/register');
    expect(offsite).toEqual([]);
  });

  test('rejects malformed booking IDs with 404', NEW_ONLY, async ({ page }) => {
    await loginGuest(page, createGuest());
    const res = await page.goto("/bookings/not-a-uuid'/change-dates");
    expect(res?.status()).toBe(404);
  });
});
