# Playwright E2E Testing Reference

## 1. Project & Port Topology

| Project Name | Portal | Base URL | Primary Test Specs |
| :--- | :--- | :--- | :--- |
| `guest-portal-chromium` | Guest Portal | `http://localhost:3000` | `playwright/e2e/guest-portal/*.spec.ts` |
| `guest-portal-firefox` | Guest Portal | `http://localhost:3000` | `playwright/e2e/guest-portal/*.spec.ts` |
| `admin-portal-chromium` | Admin Portal | `http://localhost:3002` | `playwright/e2e/admin-portal/*.spec.ts` |
| `admin-portal-firefox` | Admin Portal | `http://localhost:3002` | `playwright/e2e/admin-portal/*.spec.ts` |
| `backend-api` | Backend APIs | `http://localhost:8082` | `playwright/e2e/api/*.spec.ts` |

## 2. Infrastructure Dependencies

Playwright `globalSetup` in `playwright/e2e/setup/global-setup.ts` automatically verifies:
- **Port 5432**: PostgreSQL container `ourplaces_db`. If down, launches `devenv shell db-start`.
- **Database Seeding**: Inserts deterministic test entities (`playwright/e2e/setup/db-seed.ts`):
  - Admin user: `admin@ourplaces.io` / `admin_changeme_2026`
  - Active villa: `the-courtyard-studio-new-kingston` (Slug, USD $650, 4 guests)
- **Backend APIs**:
  - `booking_api` on `:8081` (`/health`)
  - `listing_api` on `:8082` (`/api/v1/listings/the-courtyard-studio-new-kingston`)
  - `user_api` on `:8083` (`/health`)

## 3. Topcoat Dev Server Orchestration

Handled proactively by `stack-start` / `frontends-start`:
- **Guest Portal (`:3000`)**: `cd web_app_tc && PORT=3000 topcoat dev`
- **Admin Portal (`:3002`)**: `cd web_app_admin_tc && PORT=3002 topcoat dev`
- `playwright/playwright.config.ts` configures `reuseExistingServer: true` to attach immediately to the running instances with zero startup latency.

## 4. Authentication Fixture Guidelines

- **Guest Portal**: Guest routes are public; checkout creates guest holding session in PostgreSQL.
- **Admin Portal SSR Session**: Topcoat uses `CookieTokenStore` (`op_admin_session`).
  - Use `authenticatedAdminPage` fixture from `playwright/e2e/fixtures/auth.fixture.ts`.
  - Performs live login via `/api/auth/login` to obtain real session cookie.

## 5. Report & Trace Inspection

- **Interactive HTML Report**:
  ```bash
  devenv shell npx playwright show-report playwright/playwright-report
  ```
  Launches local web server showing test steps, network calls, and DOM snapshots.
- **Headless Error Context**:
  When running in CI or non-interactive terminals, inspect:
  - `playwright/test-results/<test-folder>/error-context.md` (detailed markdown trace)
  - `playwright/test-results/<test-folder>/test-failed-1.png` (screenshot on failure)
  - `playwright/test-results/<test-folder>/video.webm` (video recording on failure)

## 6. Execution Modes (Headless vs. UI Mode)

| Mode | Flag / Command | Behavior | When to Use |
| :--- | :--- | :--- | :--- |
| **Headless (Default)** | `devenv shell test-e2e` | Runs in background without UI windows; produces console summary and test artifacts. | Default for CI/CD, agent executions, fast validation. |
| **Interactive UI** | `devenv shell test-e2e --ui` or `devenv shell test-e2e-ui` | Opens the full Playwright interactive runner with time-travel debugger, DOM inspector, and locator tester. | When explicitly requested by the user for interactive debugging. |
| **Headed** | `devenv shell test-e2e --headed` | Spawns visible browser instances during test execution. | When visually observing real-time browser rendering. |

