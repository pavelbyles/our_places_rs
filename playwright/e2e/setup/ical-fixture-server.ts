import * as fs from 'node:fs';
import * as https from 'node:https';
import * as path from 'node:path';

/**
 * A local https server for imported calendar feeds (#58). It uses the
 * certificate `ical-fixture-ca` creates; listing_api trusts that CA through
 * EA__CALENDAR_SYNC__EXTRA_ROOT_CA_PATH and may reach localhost through
 * EA__CALENDAR_SYNC__ALLOW_PRIVATE_HOSTS (both local-only, set by devenv).
 *
 * Fixtures in e2e/fixtures/ical/*.ics may use {{+N}} for "N days from
 * today" as YYYYMMDD, so their events always sit in the sync window.
 */
export const ICAL_FIXTURE_PORT = Number(process.env.ICAL_FIXTURE_PORT || 8443);
export const ICAL_FIXTURE_BASE = `https://localhost:${ICAL_FIXTURE_PORT}`;

/** YYYY-MM-DD `days` from today (UTC). */
export function fixtureDate(days: number): string {
  return new Date(Date.now() + days * 86_400_000).toISOString().slice(0, 10);
}

function render(body: string): string {
  return body.replace(/\{\{\+(\d+)\}\}/g, (_, n) => fixtureDate(Number(n)).replace(/-/g, ''));
}

/** Starts the server; resolves to a function that stops it. */
export async function startIcalFixtureServer(): Promise<() => Promise<void>> {
  const dir = process.env.ICAL_FIXTURE_DIR;
  if (!dir || !fs.existsSync(path.join(dir, 'cert.pem'))) {
    throw new Error('iCal fixture certificate missing: run `ical-fixture-ca` in the devenv shell first.');
  }
  const fixtures = path.resolve(__dirname, '../fixtures/ical');
  const server = https.createServer(
    { key: fs.readFileSync(path.join(dir, 'key.pem')), cert: fs.readFileSync(path.join(dir, 'cert.pem')) },
    (req, res) => {
      const name = path.basename((req.url || '').split('?')[0]);
      const file = path.join(fixtures, name);
      if (!name.endsWith('.ics') || !fs.existsSync(file)) {
        res.writeHead(404).end();
        return;
      }
      res.writeHead(200, { 'Content-Type': 'text/calendar; charset=utf-8' });
      res.end(render(fs.readFileSync(file, 'utf8')));
    },
  );
  await new Promise<void>((resolve, reject) => {
    server.once('error', reject);
    server.listen(ICAL_FIXTURE_PORT, '127.0.0.1', () => resolve());
  });
  console.log(`✓ iCal fixture server on ${ICAL_FIXTURE_BASE}`);
  return () => new Promise<void>((resolve) => server.close(() => resolve()));
}
