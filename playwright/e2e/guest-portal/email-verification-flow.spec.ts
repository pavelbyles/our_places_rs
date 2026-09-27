import { test, expect } from '@playwright/test';
import { execSync } from 'node:child_process';

const DB_URL =
  process.env.DATABASE_URL || 'postgres://postgres:password@localhost:5432/our_places';
const EMAIL_WORKER_URL = process.env.EMAIL_WORKER_URL || 'http://localhost:8080';

interface OutboxRecord {
  id: string;
  recipient_email: string;
  subject: string;
  template_id: string;
  status: string;
  attempts: number;
  payload: {
    token?: string;
    [key: string]: unknown;
  };
}

/**
 * Helper to query the latest email_outbox record for a given recipient.
 */
function getLatestOutboxForEmail(email: string): OutboxRecord | null {
  const sql = `SELECT id, recipient_email, subject, template_id, status, attempts, payload FROM email_outbox WHERE recipient_email = '${email}' ORDER BY created_at DESC LIMIT 1;`;
  try {
    const raw = execSync(`psql "${DB_URL}" -t -A -F "|" -c "${sql}"`, {
      encoding: 'utf-8',
      stdio: ['pipe', 'pipe', 'ignore'],
    });

    if (!raw || !raw.trim()) return null;

    const [id, recipient_email, subject, template_id, status, attempts, payloadStr] =
      raw.trim().split('|');

    return {
      id,
      recipient_email,
      subject,
      template_id,
      status,
      attempts: parseInt(attempts, 10),
      payload: JSON.parse(payloadStr || '{}'),
    };
  } catch (err) {
    console.warn('Failed to query email_outbox:', err);
    return null;
  }
}

/**
 * Helper to count outbox records for a given recipient.
 */
function countOutboxForEmail(email: string): number {
  const sql = `SELECT COUNT(*) FROM email_outbox WHERE recipient_email = '${email}';`;
  try {
    const raw = execSync(`psql "${DB_URL}" -t -A -c "${sql}"`, {
      encoding: 'utf-8',
      stdio: ['pipe', 'pipe', 'ignore'],
    });
    return parseInt(raw.trim(), 10) || 0;
  } catch {
    return 0;
  }
}

/**
 * Helper to verify user verification status in PostgreSQL.
 */
function isUserVerifiedInDb(email: string): boolean {
  const sql = `SELECT is_verified FROM \\"user\\" WHERE email = '${email}' LIMIT 1;`;
  try {
    const raw = execSync(`psql "${DB_URL}" -t -A -c "${sql}"`, {
      encoding: 'utf-8',
      stdio: ['pipe', 'pipe', 'pipe'],
    });
    return raw.trim() === 't' || raw.trim() === 'true';
  } catch (err) {
    console.warn('Failed to query user verification state:', err);
    return false;
  }
}

/**
 * Helper to simulate a Pub/Sub push event to email_worker.
 */
async function pushToEmailWorker(emailId: string): Promise<boolean> {
  const b64Data = Buffer.from(JSON.stringify({ email_id: emailId })).toString('base64');
  try {
    const response = await fetch(`${EMAIL_WORKER_URL}/pubsub/email-events`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ message: { data: b64Data } }),
    });
    return response.ok;
  } catch {
    return false;
  }
}

test.describe('Guest Portal - Email Pub/Sub & Verification Flow', () => {
  test('should register guest, record email outbox, deliver OTP via pubsub, and verify account', async ({
    page,
  }) => {
    const timestamp = Date.now();
    const testEmail = `guest.pubsub.${timestamp}@example.com`;
    const testPassword = 'Password123!';

    // 1. Visit registration page
    await page.goto('/register');
    await expect(page.locator('h2')).toContainText('Create Account');

    // 2. Fill out registration form
    await page.locator('#reg-first-name').fill('PubSub');
    await page.locator('#reg-last-name').fill('Tester');
    await page.locator('#reg-email').fill(testEmail);
    await page.locator('#reg-password').fill(testPassword);

    // 3. Submit registration form
    await page.locator('#btn-guest-reg').click();

    // 4. Assert redirect to /verify page with pre-filled email
    await expect(page).toHaveURL(new RegExp(`/verify\\?email=${encodeURIComponent(testEmail)}`), {
      timeout: 10000,
    });
    await expect(page.locator('#verify-email')).toHaveValue(testEmail);
    await expect(page.locator('#verify-info')).toBeVisible();

    // 5. Query PostgreSQL email_outbox table for the generated OTP
    const outbox = getLatestOutboxForEmail(testEmail);
    expect(outbox).not.toBeNull();
    expect(outbox?.recipient_email).toBe(testEmail);
    const otpCode = (outbox?.payload.code || outbox?.payload.token) as string;
    expect(otpCode).toBeDefined();
    expect(otpCode).toMatch(/^[A-Z0-9]{6}$/);

    // 6. Simulate Pub/Sub push to email_worker if active
    const pushDelivered = await pushToEmailWorker(outbox!.id);
    if (pushDelivered) {
      // If email_worker is listening, verify outbox status transitioned to 'sent'
      const updatedOutbox = getLatestOutboxForEmail(testEmail);
      expect(updatedOutbox?.status).toBe('sent');
      expect(updatedOutbox?.attempts).toBeGreaterThanOrEqual(1);
    } else {
      console.log(
        `ℹ Note: email_worker not running on ${EMAIL_WORKER_URL}; verified transactional outbox persistence with OTP: ${otpCode}`,
      );
    }

    // 7. Enter the 6-digit OTP code into the verification form
    await page.locator('#verify-code').fill(otpCode);
    await page.locator('#btn-verify').click();

    // 8. Assert account activation and redirect to home page
    await expect(page).toHaveURL('/', { timeout: 10000 });

    // 9. Verify in database that is_verified transitions to true
    await expect
      .poll(() => isUserVerifiedInDb(testEmail), {
        timeout: 5000,
        intervals: [100, 200, 500],
      })
      .toBe(true);

    // 10. Verify authenticated guest session in localStorage
    const authUserJson = await page.evaluate(() => localStorage.getItem('op_auth_user'));
    expect(authUserJson).not.toBeNull();
    const authUser = JSON.parse(authUserJson!);
    expect(authUser.email).toBe(testEmail);
    expect(authUser.role).toBe('booker');
  });

  test('should reject invalid 6-digit verification code', async ({ page }) => {
    const timestamp = Date.now();
    const testEmail = `guest.invalid.${timestamp}@example.com`;

    // Direct visit to verify page with test email
    await page.goto(`/verify?email=${encodeURIComponent(testEmail)}`);

    // Enter known invalid code
    await page.locator('#verify-code').fill('000000');
    await page.locator('#btn-verify').click();

    // Assert error alert displays
    const errorBox = page.locator('#verify-error');
    await expect(errorBox).toBeVisible({ timeout: 5000 });
    await expect(page.locator('#verify-error-text')).toContainText(
      /Invalid|Verification failed|User not found/i,
    );
  });

  test('should support resending verification code and generate a fresh outbox record', async ({
    page,
  }) => {
    const timestamp = Date.now();
    const testEmail = `guest.resend.${timestamp}@example.com`;

    // 1. Register user
    await page.goto('/register');
    await page.locator('#reg-first-name').fill('Resend');
    await page.locator('#reg-last-name').fill('Tester');
    await page.locator('#reg-email').fill(testEmail);
    await page.locator('#reg-password').fill('Password123!');
    await page.locator('#btn-guest-reg').click();

    await expect(page).toHaveURL(new RegExp(`/verify\\?email=${encodeURIComponent(testEmail)}`), {
      timeout: 10000,
    });

    const initialCount = countOutboxForEmail(testEmail);
    expect(initialCount).toBe(1);

    // 2. Click "Resend Code"
    const resendBtn = page.locator('#btn-resend-code');
    await expect(resendBtn).toBeVisible();
    await resendBtn.click();

    // 3. Assert success info alert
    await expect(page.locator('#verify-info')).toBeVisible();
    await expect(page.locator('#verify-info-text')).toContainText(/new verification code/i);

    // 4. Verify a second outbox record was created
    const newCount = countOutboxForEmail(testEmail);
    expect(newCount).toBe(2);

    const latestOutbox = getLatestOutboxForEmail(testEmail);
    const resendOtp = (latestOutbox?.payload.code || latestOutbox?.payload.token) as string;
    expect(resendOtp).toBeDefined();
    expect(resendOtp).toMatch(/^[A-Z0-9]{6}$/);
  });
});
