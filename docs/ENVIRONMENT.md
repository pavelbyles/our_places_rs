# Environment Variables

Every variable the services read, where it comes from in each environment, and the GitHub Actions variables and secrets that feed the deploy workflow ([`cicd_full_stack.yml`](../.github/workflows/cicd_full_stack.yml)).

- **Local**: `devenv` loads `.env` (gitignored) and sets the emulator variables itself ([`devenv.nix`](../devenv.nix)).
- **Cloud Run**: the workflow sets every variable at deploy time. Branch `main` reads the `*_PROD` variables and secrets; every other branch reads `*_DEV`.

## 1. GitHub Actions configuration

### Secrets

| Secret | Required | Used for |
| :--- | :--- | :--- |
| `GCP_PROJECT` | yes | Project ID for every `gcloud` call and image path. |
| `GCP_CREDENTIALS` | yes | Deployer service account key (Cloud Run, Artifact Registry, Pub/Sub, Storage, IAM bindings). |
| `DB_PASSWORD_DEV` / `_PROD` | yes | Cloud SQL password for the APIs and workers. |
| `JWT_SECRET_DEV` / `_PROD` | **yes (new)** | HS256 key shared by the portals (they sign user tokens) and the three APIs (they verify them). The deploy fails fast if it is missing. Generate with `openssl rand -hex 32`. |
| `SMTP_PASS_DEV` / `_PROD` | yes | SMTP password. For Gmail this is a 16-character app password, not the account password. |
| `CRON_SECRET_DEV` / `_PROD` | yes | `x-cron-secret` header for booking_api's `/api/v1/internal/cron/*` sweeps. |

### Variables

| Variable | Required | Used for |
| :--- | :--- | :--- |
| `GAR_REPO` | yes | Artifact Registry repository for images. |
| `INFRA_GCS_BUCKET_RAW_PREFIX` | yes | Raw upload bucket name prefix; the bucket is `<prefix>-<branch-slug>`. |
| `INFRA_GCS_BUCKET_PUBLIC_PREFIX` | yes | Public (resized WebP) bucket name prefix. |
| `INFRA_SA_LISTING_API_*`, `INFRA_SA_BOOKING_API_*`, `INFRA_SA_USER_API_*` | yes | Service account names (without `@project...`) for the APIs. |
| `INFRA_SA_IMAGE_WORKER_*`, `INFRA_SA_EMAIL_WORKER_*` | yes | Service accounts for the workers (email falls back to the image worker's). |
| `INFRA_SA_WEB_APP_*` | yes | Service account for both portals; granted `run.invoker` on the APIs. |
| `DB_USER_*`, `DB_NAME_*`, `DB_INSTANCE_NAME_*` | yes | Cloud SQL user, database and instance. |
| `SMTP_HOST_*`, `SMTP_USER_*`, `SMTP_FROM_*` | yes | SMTP relay for email_worker (port 587, STARTTLS). |
| `TAJ_SUPPLIER_TRN_*`, `TAJ_SUPPLIER_NAME_*`, `TAJ_SUPPLIER_ADDRESS_*` | yes | Supplier block on tax documents. Commas are allowed (the workflow escapes them). |
| `GUEST_PORTAL_URL_*`, `ADMIN_PORTAL_URL_*` | optional | Custom domains (e.g. `https://ourplaces.io`). Used for email links and the bucket CORS rule instead of the `run.app` URLs. |

No longer read by the workflow: `INFRA_GATEWAY_API_KEY_*`, `INFRA_*_API_URL_DEV`, `POSTGRES_DB_DEV`. The portals now call the APIs' Cloud Run URLs directly, because the API Gateway spec (`openapi.yaml`) does not cover the routes they use.

## 2. Per service

"Set by CI" means the deploy workflow sets the variable; you do not configure it separately.

### Portals: `web_ui_guest`, `web_ui_admin`

| Variable | Required | Local default | Cloud value |
| :--- | :--- | :--- | :--- |
| `JWT_SECRET` | yes, no fallback | `secret` (set by devenv/Playwright) | `JWT_SECRET_*` |
| `HOST` / `PORT` | no | `127.0.0.1` / `3010` (guest), `3012` (admin) | `0.0.0.0` / Cloud Run's `PORT` |
| `LISTING_API_URL`, `BOOKING_API_URL`, `USER_API_URL` | no | `http://localhost:8082`, `:8081`, `:8083` | API Cloud Run URLs (set by CI) |
| `*_API_AUDIENCE` | no | the matching `*_API_URL` | same as the URL (set by CI) |
| `ENABLE_HSTS` | no | off | `true` |
| `STORAGE_EMULATOR_HOST` | local only | `http://localhost:4443` (devenv) | unset |
| `GATEWAY_API_KEY` | no | unset | unset (only sent if present) |
| `RUST_LOG`, `LOG_FORMAT` | no | | `info`, `json` |

The portals call the APIs with two credentials: the service account's Google ID token in `Authorization` (Cloud Run IAM) and the signed-in user's JWT in `X-Forwarded-Authorization`. On Cloud Run the ID token comes from the metadata server; locally `gcloud auth print-identity-token` is tried and skipped if unavailable.

### APIs: `listing_api`, `booking_api`, `user_api`

Settings load from `config/Default.toml`, then `config/<RUN_ENV>.toml`, then `EA__*` variables (`EA__DATABASE__PASSWORD` overrides `database.password`).

| Variable | Service | Required | Notes |
| :--- | :--- | :--- | :--- |
| `EA__SERVER__HOST`, `EA__SERVER__PORT` | all | cloud | `0.0.0.0`, `8080` |
| `EA__DATABASE__*` (`CLOUD`, `INSTANCE_NAME`, `USERNAME`, `PASSWORD`, `DATABASE_NAME`, `DATABASE_URL`) | all | cloud | `DATABASE_URL` sets `search_path` to the branch schema. |
| `RUN_ENV` | all | no | Selects `config/<RUN_ENV>.toml`. |
| `JWT_SECRET` | all | **yes in cloud** | Must match the portals. Falls back to `secret` if unset, which anyone could use to forge tokens. |
| `EMAIL_PUBSUB_TOPIC_ID`, `GCP_PROJECT_ID` | booking, user | yes | `our-places-email-notifications-<branch>` and the project. |
| `PUBSUB_EMULATOR_HOST` | booking, user | local only | `127.0.0.1:8085`. Unset in the cloud, where publishes use the metadata server's OAuth token. |
| `DISABLE_PUBSUB_PUBLISH`, `PUBSUB_AUTH_TOKEN` | booking, user | no | Tests / manual overrides. Leave unset in the cloud. |
| `GCS_RAW_BUCKET` | listing | yes | Bucket that signed upload URLs point at. |
| `STORAGE_EMULATOR_HOST` | listing | local only | Fake GCS for signed URLs. |
| `BASE_URL` | booking | yes in cloud | Guest portal URL for links in emails. Set by CI after the portals deploy. Local default `http://localhost:3010`. |
| `ADMIN_BASE_URL` | booking | yes in cloud | Admin portal URL for host links in emails. Set by CI. Local default `http://localhost:3012`. |
| `CRON_SECRET` | booking | yes | Required by the cron endpoints; no fallback. |
| `TAJ_SUPPLIER_TRN`, `TAJ_SUPPLIER_NAME`, `TAJ_SUPPLIER_ADDRESS` | booking | yes | Tax document supplier block. |
| `ACTUATOR_SECRET` (or `ADMIN_SECRET`) | all | recommended | `X-Actuator-Token` for `/loggers`. Falls back to `secret`. Not set by CI today. Never set it to an empty string. |

### Workers

| Variable | Service | Required | Notes |
| :--- | :--- | :--- | :--- |
| `EA__*` database settings | both | cloud | As for the APIs. |
| `SMTP_HOST`, `SMTP_PORT` | email | yes | `smtp.gmail.com`, `587` (STARTTLS; `465` uses implicit TLS). |
| `SMTP_USERNAME` (or `SMTP_USER`) | email | yes | SMTP login. |
| `SMTP_PASSWORD` (or `SMTP_PASS`) | email | yes | Gmail app password. |
| `SMTP_FROM_EMAIL` (or `SMTP_FROM`) | email | yes | Sender. With Gmail it must be the account itself or a verified "Send mail as" alias, or Gmail rewrites it. |
| `USE_MOCK_EMAIL_PROVIDER` | email | no | Defaults to mock only when `SMTP_HOST` is unset. CI sets `false` so a missing host fails loudly instead of silently mocking. |
| `MAX_EMAIL_RETRIES` | email | no | Default `3`. |
| `PUBSUB_SECRET_TOKEN` | email | no | Leave **unset** in the cloud: Pub/Sub sends its OIDC token in `Authorization`, which would not match. |
| `EMAIL_TEMPLATES_DIR` | email | no | Defaults to `/app/templates` in the image. |
| `GCS_RAW_BUCKET`, `GCS_PUBLIC_BUCKET` | image | yes | Source and destination buckets. |
| `STORAGE_EMULATOR_HOST` | image | local only | Fake GCS. |

## 3. Cloud wiring the workflow creates per branch

- **Image pipeline**: raw bucket → `OBJECT_FINALIZE` notification → topic `our-places-image-uploads-raw-<branch>` → push subscription (OIDC, `INFRA_SA_IMAGE_WORKER`) → image worker `/api/v1/internal/image/process_image` → public bucket.
- **Browser uploads**: the admin job sets a CORS rule on the raw bucket that allows `PUT` from the admin portal's origin (and `ADMIN_PORTAL_URL_*` if set). Signed URLs are created by `INFRA_SA_LISTING_API`, which needs `roles/iam.serviceAccountTokenCreator` on itself and `roles/storage.objectCreator` on the raw bucket. The bucket-setup job grants both (the prod service account had neither).
- **Email pipeline**: booking/user API write an outbox row → publish to `our-places-email-notifications-<branch>` (the API service accounts get `pubsub.publisher` on the topic) → push subscription (OIDC, `INFRA_SA_EMAIL_WORKER`) → email worker `/pubsub/email-events` → SMTP.
- **Scheduled sweeps**: on `dev` and `main` only, the API job creates or updates one Cloud Scheduler job, `our-places-sweeps-<branch>`. It fires hourly at :05 and calls booking_api's `POST /api/v1/internal/cron/run-sweeps` with `x-cron-secret` and an OIDC token for `INFRA_SA_BOOKING_API` (granted `run.invoker` on booking_api). Each run sends the due pre-arrival guides, host arrival reminders and expiring-hold warnings, and completes stays from noon Jamaica time on their checkout day. That makes 2 jobs in total, within the free tier of 3 jobs per billing account; feature branches get none. The older `process-scheduled-notifications` and `complete-stays` endpoints still exist for manual runs.
