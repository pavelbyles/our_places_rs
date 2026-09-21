---
name: playwright-e2e
description: Run Playwright end-to-end tests across Topcoat guest and admin portals, inspect failure traces, and view HTML reports.
---

# Playwright E2E Testing

Execute, debug, and report end-to-end tests for the Topcoat SSR frontend applications (`web_app_tc` and `web_app_admin_tc`) against live backend services.

## Core Workflow

1. **Pre-flight Readiness**:
   - `test-e2e` automatically invokes `stack-start` (which ensures PostgreSQL via `db-start`, boots `booking_api`, `listing_api`, and `user_api` in background if offline via `apis-start`, and boots `web_app_tc` and `web_app_admin_tc` via `frontends-start`).
   - All services are pre-warmed and health-checked before test execution begins.

2. **Execution Mode Selection (Headless vs. UI)**:
   - **Headless Mode (Default)**: Execute tests headless by default or when the user specifies headless. Omit `--ui` or `--headed`.
   - **Interactive UI Mode**: When the user specifies UI mode (e.g., "with UI", "launch UI", "in UI"), append `--ui` or run `test-e2e-ui`.
   - **Headed Mode**: When the user asks to see browser windows during execution, append `--headed`.

3. **Run Test Suites**:
   - **Headless Runs (Default)**:
     - Guest Portal (Chromium):
       ```bash
       test-e2e --project=guest-portal-chromium
       ```
     - Admin Portal (Chromium):
       ```bash
       test-e2e --project=admin-portal-chromium
       ```
     - Full Cross-Browser Run (Chromium & Firefox):
       ```bash
       test-e2e
       ```
     - Direct API Sanity:
       ```bash
       test-e2e --project=backend-api
       ```

   - **Interactive UI Runs (When User Requests UI)**:
     - Full UI Runner:
       ```bash
       test-e2e --ui
       ```
     - Guest Portal in UI:
       ```bash
       test-e2e --ui --project=guest-portal-chromium
       ```
     - Admin Portal in UI:
       ```bash
       test-e2e --ui --project=admin-portal-chromium
       ```

4. **Inspect Results & Show Report**:
   - Review terminal output for test execution times, assertions, and status.
   - For failed tests, inspect the generated markdown report in `playwright/test-results/<test-id>/error-context.md` and failure screenshots in `playwright/test-results/`.
   - To launch the interactive HTML report viewer:
     ```bash
     npx playwright show-report playwright/playwright-report
     ```

## Verification Checklist

- [ ] PostgreSQL container is up and seeded with test data (`admin@ourplaces.io`, `the-courtyard-studio-new-kingston`).
- [ ] Backend microservices return HTTP 200 on health/listing endpoints.
- [ ] No fallback masking (`if (await notFound.isVisible())`) present in test specs.
- [ ] All assertions pass without strict mode locator collisions.

---

## Detailed References
* For project matrix, fixture setup, authentication state caching, and troubleshooting, see **[REFERENCE.md](REFERENCE.md)**.
