import { defineConfig, devices } from '@playwright/test';

/**
 * See https://playwright.dev/docs/test-configuration.
 */
export default defineConfig({
  testDir: './e2e',
  /* Maximum time one test can run for. */
  timeout: 30 * 1000,
  expect: {
    timeout: 5000,
  },
  /* Run tests in files in parallel */
  fullyParallel: true,
  /* Fail the build on CI if you accidentally left test.only in the source code. */
  forbidOnly: !!process.env.CI,
  /* Retry on CI only */
  retries: process.env.CI ? 2 : 0,
  /* Opt out of parallel tests on CI if needed, or keep 2-4 workers */
  workers: process.env.CI ? 2 : undefined,
  /* Reporter to use. See https://playwright.dev/docs/test-reporters */
  reporter: [
    ['list'],
    ['html', { outputFolder: 'playwright-report', open: 'never' }],
  ],

  /* Global setup to verify DB and backend microservices */
  globalSetup: './e2e/setup/global-setup.ts',

  /* Shared settings for all the projects below. See https://playwright.dev/docs/api/class-testoptions. */
  use: {
    /* Collect trace when retrying the failed test. See https://playwright.dev/docs/trace-viewer */
    trace: 'on-first-retry',
    screenshot: 'only-on-failure',
    video: 'retain-on-failure',
  },

  /* Configure projects for major browsers and portals */
  projects: [
    // ----------------------------------------------------
    // Guest Portal (web_app_tc on :3000)
    // ----------------------------------------------------
    {
      name: 'guest-portal-chromium',
      testDir: './e2e/guest-portal',
      use: {
        ...devices['Desktop Chrome'],
        baseURL: 'http://localhost:3000',
      },
    },
    {
      name: 'guest-portal-firefox',
      testDir: './e2e/guest-portal',
      use: {
        ...devices['Desktop Firefox'],
        baseURL: 'http://localhost:3000',
      },
    },

    // ----------------------------------------------------
    // Admin Portal (web_app_admin_tc on :3002)
    // ----------------------------------------------------
    {
      name: 'admin-portal-chromium',
      testDir: './e2e/admin-portal',
      use: {
        ...devices['Desktop Chrome'],
        baseURL: 'http://localhost:3002',
      },
    },
    {
      name: 'admin-portal-firefox',
      testDir: './e2e/admin-portal',
      use: {
        ...devices['Desktop Firefox'],
        baseURL: 'http://localhost:3002',
      },
    },
    // ----------------------------------------------------
    // Backend API Validation
    // ----------------------------------------------------
    {
      name: 'backend-api',
      testDir: './e2e/api',
    },
  ],

  /* Run local Topcoat web servers if they aren't already running */
  webServer: [
    {
      command: 'cd web_app_tc && topcoat dev',
      url: 'http://localhost:3000',
      reuseExistingServer: !process.env.CI,
      stdout: 'ignore',
      stderr: 'pipe',
      timeout: 60 * 1000,
    },
    {
      command: 'cd web_app_admin_tc && PORT=3002 topcoat dev',
      url: 'http://localhost:3002',
      reuseExistingServer: !process.env.CI,
      stdout: 'ignore',
      stderr: 'pipe',
      timeout: 60 * 1000,
    },
  ],
});
