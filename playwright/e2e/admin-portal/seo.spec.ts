import { test, expect } from '../fixtures/base.fixture.js';

// Spec 102 §1.3 / §1.6: the admin portal is never indexable.
const NEW_ONLY = { tag: '@new-only' };

test.describe('Admin Portal - SEO (spec 102)', () => {
  test('robots.txt disallows everything without signing in', NEW_ONLY, async ({ request }) => {
    const res = await request.get('/robots.txt', { maxRedirects: 0 });
    expect(res.status()).toBe(200);
    expect(res.headers()['content-type']).toContain('text/plain');
    expect(await res.text()).toBe('User-agent: *\nDisallow: /\n');
  });

  test('every admin response is noindex, including errors', NEW_ONLY, async ({ request }) => {
    for (const p of ['/robots.txt', '/login', '/no-such-page']) {
      const res = await request.get(p, { maxRedirects: 0 });
      expect(res.headers()['x-robots-tag'], p).toBe('noindex, nofollow');
    }
  });
});
