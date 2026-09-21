import { defineConfig, devices } from '@playwright/test';
import * as path from 'node:path';

const currentDir = __dirname;
const rootDir = path.resolve(currentDir, '..');

/**
 * See https://playwright.dev/docs/test-configuration.
 */
export default defineConfig({
  testDir: path.resolve(currentDir, 'e2e'),
  outputDir: path.resolve(currentDir, 'test-results'),
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
    ['html', { outputFolder: path.resolve(__dirname, 'playwright-report'), open: 'never' }],
  ],

  /* Global setup to verify DB and backend microservices */
  globalSetup: path.resolve(__dirname, 'e2e/setup/global-setup.ts'),

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
      testDir: path.resolve(__dirname, 'e2e/guest-portal'),
      use: {
        ...devices['Desktop Chrome'],
        baseURL: 'http://localhost:3000',
      },
    },
    {
      name: 'guest-portal-firefox',
      testDir: path.resolve(__dirname, 'e2e/guest-portal'),
      use: {
        ...devices['Desktop Firefox'],
        baseURL: 'http://localhost:3000',
        launchOptions: {
          env: {
            ...process.env,
            LD_LIBRARY_PATH: `/usr/lib/x86_64-linux-gnu:/lib/x86_64-linux-gnu:${process.env.LD_LIBRARY_PATH || ''}`,
          },
        },
      },
    },

    // ----------------------------------------------------
    // Admin Portal (web_app_admin_tc on :3002)
    // ----------------------------------------------------
    {
      name: 'admin-portal-chromium',
      testDir: path.resolve(__dirname, 'e2e/admin-portal'),
      use: {
        ...devices['Desktop Chrome'],
        baseURL: 'http://localhost:3002',
      },
    },
    {
      name: 'admin-portal-firefox',
      testDir: path.resolve(__dirname, 'e2e/admin-portal'),
      use: {
        ...devices['Desktop Firefox'],
        baseURL: 'http://localhost:3002',
        launchOptions: {
          env: {
            ...process.env,
            LD_LIBRARY_PATH: `/usr/lib/x86_64-linux-gnu:/lib/x86_64-linux-gnu:${process.env.LD_LIBRARY_PATH || ''}`,
          },
        },
      },
    },
    // ----------------------------------------------------
    // Backend API Validation
    // ----------------------------------------------------
    {
      name: 'backend-api',
      testDir: path.resolve(__dirname, 'e2e/api'),
    },
  ],

  /* Run local Topcoat web servers if they aren't already running */
  webServer: [
    {
      command: 'cd web_app_tc && topcoat dev',
      cwd: rootDir,
      url: 'http://localhost:3000',
      reuseExistingServer: !process.env.CI,
      stdout: 'ignore',
      stderr: 'pipe',
      timeout: 120 * 1000,
    },
    {
      command: 'cd web_app_admin_tc && PORT=3002 topcoat dev',
      cwd: rootDir,
      url: 'http://localhost:3002',
      reuseExistingServer: !process.env.CI,
      stdout: 'ignore',
      stderr: 'pipe',
      timeout: 120 * 1000,
    },
  ],
});
