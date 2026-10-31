import * as crypto from 'node:crypto';
import type { Page } from '@playwright/test';
import { psql } from './bookings-data.js';
import { TEST_USERS } from './test-data.js';

/** bcrypt hash of TEST_USERS.admin.password, as seeded in db-seed.ts. */
const ADMIN_PASSWORD_HASH = '$2b$12$H09UlIMmYBCR3TpiZ44.SOkrYELsOkplVxMYH6Aup52FbrMF5VGCW';

export interface TestHost {
  id: string;
  email: string;
  password: string;
}

/** Creates a verified host (no admin role) that only this test uses. */
export function createHost(): TestHost {
  const id = crypto.randomUUID();
  const email = `e2e.host.${id.slice(0, 8)}@example.com`;
  psql(`
    INSERT INTO "user" (id, email, password_hash, first_name, last_name, roles,
                        is_active, is_verified, default_currency, created_at, updated_at)
    VALUES ('${id}', '${email}', '${ADMIN_PASSWORD_HASH}', 'Harper', 'Host',
            '{host,booker}', true, true, 'USD', NOW(), NOW());
    INSERT INTO host_profiles (user_id, verified_status) VALUES ('${id}', 'pending');
  `);
  return { id, email, password: TEST_USERS.admin.password };
}

export async function loginAdminAs(page: Page, user: { email: string; password: string }): Promise<void> {
  await page.goto('/login');
  await page.locator('#admin-login-email').fill(user.email);
  await page.locator('#admin-login-password').fill(user.password);
  await page.locator('#btn-admin-login').click();
  await page.waitForFunction(() => !window.location.pathname.includes('/login'), { timeout: 15000 });
}

export function userVerificationCode(email: string): string {
  return psql(`SELECT verification_code FROM "user" WHERE email = '${email}';`);
}
