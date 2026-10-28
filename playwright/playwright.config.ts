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
      grepInvert: /@new-only/,
      testDir: path.resolve(__dirname, 'e2e/guest-portal'),
      use: {
        ...devices['Desktop Chrome'],
        baseURL: 'http://localhost:3000',
      },
    },
    {
      name: 'guest-portal-firefox',
      grepInvert: /@new-only/,
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
      grepInvert: /@new-only/,
      testDir: path.resolve(__dirname, 'e2e/admin-portal'),
      use: {
        ...devices['Desktop Chrome'],
        baseURL: 'http://localhost:3002',
      },
    },
    {
      name: 'admin-portal-firefox',
      grepInvert: /@new-only/,
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
    // Redesigned portals (web_ui_guest on :3010, web_ui_admin on :3012).
    // Same spec files as the legacy projects; tests tagged @old-only are
    // skipped, and base.fixture fails any test with a CSP violation.
    // ----------------------------------------------------
    {
      name: 'guest-portal-new-chromium',
      testDir: path.resolve(__dirname, 'e2e/guest-portal'),
      grepInvert: /@old-only/,
      use: {
        ...devices['Desktop Chrome'],
        baseURL: 'http://localhost:3010',
      },
    },
    {
      name: 'guest-portal-new-firefox',
      testDir: path.resolve(__dirname, 'e2e/guest-portal'),
      grepInvert: /@old-only/,
      use: {
        ...devices['Desktop Firefox'],
        baseURL: 'http://localhost:3010',
        launchOptions: {
          env: {
            ...process.env,
            LD_LIBRARY_PATH: `/usr/lib/x86_64-linux-gnu:/lib/x86_64-linux-gnu:${process.env.LD_LIBRARY_PATH || ''}`,
          },
        },
      },
    },
    {
      name: 'admin-portal-new-chromium',
      testDir: path.resolve(__dirname, 'e2e/admin-portal'),
      grepInvert: /@old-only/,
      use: {
        ...devices['Desktop Chrome'],
        baseURL: 'http://localhost:3012',
      },
    },
    {
      name: 'admin-portal-new-firefox',
      testDir: path.resolve(__dirname, 'e2e/admin-portal'),
      grepInvert: /@old-only/,
      use: {
        ...devices['Desktop Firefox'],
        baseURL: 'http://localhost:3012',
        launchOptions: {
          env: {
            ...process.env,
            LD_LIBRARY_PATH: `/usr/lib/x86_64-linux-gnu:/lib/x86_64-linux-gnu:${process.env.LD_LIBRARY_PATH || ''}`,
          },
        },
      },
    },
    // ----------------------------------------------------
    // web_ui_common/src/assets/ui.js in a bare page (no server needed).
    // ----------------------------------------------------
    {
      name: 'ui-js-chromium',
      testDir: path.resolve(__dirname, 'e2e/ui-js'),
      use: { ...devices['Desktop Chrome'] },
    },
    {
      name: 'ui-js-firefox',
      testDir: path.resolve(__dirname, 'e2e/ui-js'),
      use: {
        ...devices['Desktop Firefox'],
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
      command: 'cd web_app_tc && PORT=3000 CARGO_TARGET_DIR=../target/guest topcoat dev',
      cwd: rootDir,
      url: 'http://localhost:3000',
      reuseExistingServer: !process.env.CI,
      stdout: 'pipe',
      stderr: 'pipe',
      timeout: 180 * 1000,
    },
    {
      command: 'cd web_app_admin_tc && PORT=3002 CARGO_TARGET_DIR=../target/admin topcoat dev',
      cwd: rootDir,
      url: 'http://localhost:3002',
      reuseExistingServer: !process.env.CI,
      stdout: 'pipe',
      stderr: 'pipe',
      timeout: 180 * 1000,
    },
    {
      command: 'cd web_ui_guest && PORT=3010 CARGO_TARGET_DIR=../target/ui_guest topcoat dev',
      // The new portals refuse to start without JWT_SECRET. Local backends
      // fall back to "secret" when it is unset, so match that here.
      env: { ...process.env, JWT_SECRET: process.env.JWT_SECRET || 'secret' } as Record<string, string>,
      cwd: rootDir,
      url: 'http://localhost:3010',
      reuseExistingServer: !process.env.CI,
      stdout: 'pipe',
      stderr: 'pipe',
      timeout: 180 * 1000,
    },
    {
      command: 'cd web_ui_admin && PORT=3012 CARGO_TARGET_DIR=../target/ui_admin topcoat dev',
      // The new portals refuse to start without JWT_SECRET. Local backends
      // fall back to "secret" when it is unset, so match that here.
      env: { ...process.env, JWT_SECRET: process.env.JWT_SECRET || 'secret' } as Record<string, string>,
      cwd: rootDir,
      url: 'http://localhost:3012',
      reuseExistingServer: !process.env.CI,
      stdout: 'pipe',
      stderr: 'pipe',
      timeout: 180 * 1000,
    },
  ],
});
