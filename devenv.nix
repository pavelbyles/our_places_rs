{ pkgs, ... }:

{
  # Automatically load .env (DATABASE_URL, JWT_SECRET, etc.)
  dotenv.enable = true;

  # Publish outbox emails to the local emulator started by `pubsub-start`,
  # not Google Cloud (which rejects unauthenticated local publishes with 403).
  env.PUBSUB_EMULATOR_HOST = "127.0.0.1:8085";

  # Store listing photos in the local Cloud Storage emulator started by
  # `gcs-start` (fake-gcs-server), not Google Cloud. Uploads there publish to
  # the Pub/Sub emulator, which pushes them to the local image_worker.
  env.STORAGE_EMULATOR_HOST = "http://localhost:4443";
  env.GCS_RAW_BUCKET = "our-places-gcs-img-raw-local";
  env.GCS_PUBLIC_BUCKET = "our-places-gcs-img-public-local";

  # Rust Toolchain pinned to match the Dockerfiles (rustc, cargo, clippy,
  # rustfmt, rust-analyzer). Topcoat 0.11 requires rustc 1.99.
  languages.rust = {
    enable = true;
    channel = "stable";
    version = "1.99.1";
  };

  # JavaScript/Node.js Toolchain (node & npm for DaisyUI / Tailwind)
  languages.javascript = {
    enable = true;
    npm = {
      enable = true;
      install.enable = true;
    };
  };

  # Python Toolchain (for .agents/evals/eval_runner.py)
  languages.python.enable = true;

  # Java Toolchain (OpenJDK 21 required for local Google Cloud Pub/Sub emulator)
  languages.java = {
    enable = true;
    jdk.package = pkgs.openjdk21;
  };

  # Native dependencies & developer CLI utilities
  packages = [
    pkgs.sqlx-cli
    pkgs.cargo-watch
    pkgs.cargo-audit
    pkgs.ripgrep
    pkgs.pkg-config
    pkgs.openssl
    pkgs.postgresql
    pkgs.psmisc
    pkgs.curl
    pkgs.jq
    (pkgs.google-cloud-sdk.withExtraComponents [ pkgs.google-cloud-sdk.components.pubsub-emulator ])
    pkgs.fake-gcs-server
  ];

  # Fast git pre-commit formatting check
  git-hooks.hooks = {
    rustfmt.enable = true;
  };

  # Native PostgreSQL 18 Service (matches CloudSQL PostgreSQL 18)
  services.postgres = {
    enable = true;
    package = pkgs.postgresql_18;
    listen_addresses = "127.0.0.1";
    port = 5432;
    initialDatabases = [
      {
        name = "our_places";
        user = "postgres";
        pass = "password";
      }
    ];
    initialScript = "ALTER USER postgres WITH SUPERUSER CREATEDB;";
  };

  # Project workflow commands mirroring .agents/ workflows
  scripts = {
    # Database start, stop, migrations & metadata (devenv-managed PostgreSQL 18)
    db-start.exec = ''
      if ! pg_isready -h localhost -p 5432 >/dev/null 2>&1; then
        echo "Starting devenv PostgreSQL 18 service..."
        devenv up -d postgres
      fi
      until pg_isready -h localhost -p 5432 >/dev/null 2>&1; do
        sleep 1
      done
      echo "✓ PostgreSQL is accepting connections on localhost:5432"
    '';

    db-stop.exec = ''
      echo "Stopping devenv PostgreSQL 18 service..."
      devenv processes stop postgres 2>/dev/null || true
      echo "✓ Database stopped."
    '';

    # Docker fallback start/stop scripts
    docker-db-start.exec = ''
      if ! pg_isready -h localhost -p 5432 >/dev/null 2>&1; then
        echo "Starting Docker container 'ourplaces_db'..."
        docker start ourplaces_db 2>/dev/null || docker compose up -d db
      fi
      until pg_isready -h localhost -p 5432 >/dev/null 2>&1; do
        sleep 1
      done
      echo "PostgreSQL is accepting connections on localhost:5432"
    '';

    docker-db-stop.exec = ''
      echo "Stopping Docker container 'ourplaces_db'..."
      docker stop ourplaces_db 2>/dev/null || true
      echo "✓ Database stopped."
    '';

    db-seed.exec = ''
      echo "Seeding database with initial admin user..."
      cargo run -p db_core --bin seed-db -- "$@"
    '';

    db-migrate.exec = ''
      echo "Running sqlx migrations from db_core/migrations..."
      sqlx migrate run --source db_core/migrations
    '';

    db-prepare.exec = ''
      echo "Refreshing sqlx offline query metadata..."
      cargo sqlx prepare --workspace -- --all-targets
    '';

    check-all.exec = ''
      echo "Running workspace clippy and test suites..."
      cargo clippy --workspace && cargo test --workspace
    '';

    # Pre-PR Sanity Check (.agents/workflows/sanity-check-workflow.md)
    sanity-check.exec = ''
      set -e
      echo "=== [1/5] Checking Frontend Dependencies ==="
      npm ci --silent
      echo "=== [2/5] Checking Code Formatting ==="
      cargo fmt --check
      echo "=== [3/5] Running Offline Clippy with Zero Warnings ==="
      SQLX_OFFLINE=true cargo clippy --workspace --exclude protoproj --all-features --manifest-path Cargo.toml -- -D warnings
      echo "=== [4/5] Checking Offline Target Compilation ==="
      SQLX_OFFLINE=true cargo check --workspace --exclude protoproj --all-features --tests
      echo "=== [5/5] Running Isolated CI Test Matrix ==="
      test-ci-matrix
      echo "✅ Sanity check passed cleanly!"
    '';

    # Isolated CI Test Matrix (mirrors GitHub Actions jobs)
    test-ci-matrix.exec = ''
      set -e
      echo "--> Running Backend & Shared Domain Tests (mirrors test-api)..."
      cargo test --workspace --exclude web_client --exclude web_ui_common --exclude web_ui_guest --exclude web_ui_admin --exclude protoproj
      echo "--> Running Web UI Tests (mirrors test-web-ui)..."
      cargo test -p web_client -p web_ui_common -p web_ui_guest -p web_ui_admin
    '';

    # Float-Ban & Booking Logic Audit (.agents/workflows/audit-booking-flow.md)
    audit-booking.exec = ''
      echo "Checking for illegal floating-point types (f32/f64) in pricing & booking..."
      if rg --type rust "f32|f64" common/src/pricing.rs app_api/booking_api/; then
        echo "❌ Hard invariant violation: Found float usage in financial context!"
        exit 1
      else
        echo "✓ No floating point types found in monetary modules."
      fi
      echo "Running booking domain unit tests..."
      cargo test -p common -p booking_api
    '';

    # Security Audit (.agents/workflows/sanity-check-workflow.md)
    security-audit.exec = ''
      echo "Auditing dependencies for known vulnerabilities..."
      cargo audit "$@"
    '';

    # AI Skill Benchmark Evaluation (.agents/workflows/eval-skills.md)
    eval-skills.exec = ''
      echo "Running AI skill benchmark assertions..."
      python3 .agents/evals/eval_runner.py "$@"
    '';

    # GCP Pub/Sub Emulator start, stop & topic/subscription provisioning
    pubsub-start.exec = ''
      if ! (echo > /dev/tcp/127.0.0.1/8085) >/dev/null 2>&1; then
        echo "Starting Pub/Sub emulator on 127.0.0.1:8085..."
        mkdir -p /tmp/pubsub_emulator
        nohup gcloud beta emulators pubsub start --host-port=127.0.0.1:8085 --data-dir=/tmp/pubsub_emulator > /tmp/pubsub_emulator.log 2>&1 &
        echo $! > /tmp/pubsub_emulator.pid
      fi
      retries=15
      until (echo > /dev/tcp/127.0.0.1/8085) >/dev/null 2>&1 || [ $retries -eq 0 ]; do
        sleep 1
        retries=$((retries - 1))
      done
      if (echo > /dev/tcp/127.0.0.1/8085) >/dev/null 2>&1; then
        echo "✓ Pub/Sub emulator is listening on 127.0.0.1:8085"
        echo "Ensuring topic and push subscription exist..."
        curl -s -X PUT http://127.0.0.1:8085/v1/projects/our-places-dev/topics/email-notifications-topic >/dev/null 2>&1 || true
        curl -s -X PUT http://127.0.0.1:8085/v1/projects/our-places-dev/subscriptions/email-worker-sub \
          -H "Content-Type: application/json" \
          -d '{"topic":"projects/our-places-dev/topics/email-notifications-topic","ackDeadlineSeconds":60,"pushConfig":{"pushEndpoint":"http://127.0.0.1:8080/pubsub/email-events"}}' >/dev/null 2>&1 || true
        echo "✓ Pub/Sub topic [email-notifications-topic] and push subscription [email-worker-sub -> :8080] ready."
        # Raw photo uploads (published by fake-gcs-server). Resizing a photo
        # in a debug build can take a while, hence the long ack deadline.
        curl -s -X PUT http://127.0.0.1:8085/v1/projects/our-places-dev/topics/gcs-image-uploads >/dev/null 2>&1 || true
        curl -s -X PUT http://127.0.0.1:8085/v1/projects/our-places-dev/subscriptions/image-worker-sub \
          -H "Content-Type: application/json" \
          -d '{"topic":"projects/our-places-dev/topics/gcs-image-uploads","ackDeadlineSeconds":300,"pushConfig":{"pushEndpoint":"http://127.0.0.1:8084/api/v1/internal/image/process_image"}}' >/dev/null 2>&1 || true
        echo "✓ Pub/Sub topic [gcs-image-uploads] and push subscription [image-worker-sub -> :8084] ready."
      else
        echo "❌ Failed to start Pub/Sub emulator. Check /tmp/pubsub_emulator.log"
      fi
    '';

    pubsub-stop.exec = ''
      echo "Stopping Pub/Sub emulator..."
      if [ -f /tmp/pubsub_emulator.pid ]; then
        PID=$(cat /tmp/pubsub_emulator.pid)
        kill $PID 2>/dev/null || true
        rm -f /tmp/pubsub_emulator.pid
      fi
      pkill -f "cloud-pubsub-emulator.*8085" 2>/dev/null || true
      echo "✓ Pub/Sub emulator stopped."
    '';

    # Local Cloud Storage emulator (fake-gcs-server) on localhost:4443. Data
    # persists under .devenv/state/fake-gcs. Run after `pubsub-start`: raw
    # uploads publish to the [gcs-image-uploads] topic.
    gcs-start.exec = ''
      GCS_DIR="$DEVENV_STATE/fake-gcs"
      mkdir -p "$GCS_DIR/data"
      # Throwaway key for signing upload URLs locally. The emulator doesn't
      # check signatures, so no real service-account key is needed.
      if [ ! -f "$GCS_DIR/signing-key.json" ]; then
        openssl genpkey -algorithm RSA -pkeyopt rsa_keygen_bits:2048 -out "$GCS_DIR/signing-key.pem" 2>/dev/null
        jq -n --rawfile key "$GCS_DIR/signing-key.pem" '{
          type: "service_account",
          project_id: "our-places-dev",
          private_key_id: "local-fake-gcs",
          private_key: $key,
          client_email: "local-dev@our-places-dev.iam.gserviceaccount.com",
          client_id: "0",
          token_uri: "https://oauth2.googleapis.com/token"
        }' > "$GCS_DIR/signing-key.json"
        chmod 600 "$GCS_DIR/signing-key.json"
        rm -f "$GCS_DIR/signing-key.pem"
      fi
      if ! (echo > /dev/tcp/127.0.0.1/4443) >/dev/null 2>&1; then
        echo "Starting Cloud Storage emulator (fake-gcs-server) on localhost:4443..."
        nohup fake-gcs-server -scheme http -host 127.0.0.1 -port 4443 \
          -public-host localhost:4443 -external-url http://localhost:4443 \
          -backend filesystem -filesystem-root "$GCS_DIR/data" \
          -event.pubsub-project-id our-places-dev -event.pubsub-topic gcs-image-uploads \
          -event.bucket "$GCS_RAW_BUCKET" -cors-headers content-type \
          > /tmp/fake_gcs.log 2>&1 &
        echo $! > /tmp/fake_gcs.pid
      fi
      retries=15
      until (echo > /dev/tcp/127.0.0.1/4443) >/dev/null 2>&1 || [ $retries -eq 0 ]; do
        sleep 1
        retries=$((retries - 1))
      done
      if (echo > /dev/tcp/127.0.0.1/4443) >/dev/null 2>&1; then
        echo "✓ Cloud Storage emulator is listening on localhost:4443"
        for bucket in "$GCS_RAW_BUCKET" "$GCS_PUBLIC_BUCKET"; do
          curl -s -X POST "http://localhost:4443/storage/v1/b?project=our-places-dev" \
            -H "Content-Type: application/json" -d "{\"name\":\"$bucket\"}" >/dev/null 2>&1 || true
        done
        echo "✓ Buckets [$GCS_RAW_BUCKET] and [$GCS_PUBLIC_BUCKET] ready."
      else
        echo "❌ Failed to start Cloud Storage emulator. Check /tmp/fake_gcs.log"
      fi
    '';

    gcs-stop.exec = ''
      echo "Stopping Cloud Storage emulator..."
      if [ -f /tmp/fake_gcs.pid ]; then
        kill "$(cat /tmp/fake_gcs.pid)" 2>/dev/null || true
        rm -f /tmp/fake_gcs.pid
      fi
      pkill -f "fake-gcs-server.*4443" 2>/dev/null || true
      echo "✓ Cloud Storage emulator stopped."
    '';

    # Launch all API microservices in foreground (DB is kept running continuously)
    apis.exec = ''
      db-start
      pubsub-start
      gcs-start
      echo "Starting API microservices (listing_api, booking_api, user_api, email_worker, image_worker)..."
      devenv up listing_api booking_api user_api email_worker image_worker "$@"
    '';

    # Ensure database and API microservices are active in background (detached)
    apis-start.exec = ''
      db-start
      db-migrate
      pubsub-start
      gcs-start
      echo "Starting API microservices in background (listing_api, booking_api, user_api, email_worker, image_worker)..."
      devenv up -d listing_api booking_api user_api email_worker image_worker "$@"
    '';

    # Stop API microservices
    apis-stop.exec = ''
      echo "Stopping API microservices..."
      devenv processes stop listing_api || true
      devenv processes stop booking_api || true
      devenv processes stop user_api || true
      devenv processes stop email_worker || true
      devenv processes stop image_worker || true
      gcs-stop
      pubsub-stop
    '';

    # Launch the Topcoat UI portals with topcoat dev
    frontends.exec = ''
      echo "Starting portals (web_ui_guest on :3010, web_ui_admin on :3012)..."
      devenv up web_ui_guest web_ui_admin "$@"
    '';

    # Stop the Topcoat UI portals
    frontends-stop.exec = ''
      echo "Stopping portals..."
      devenv processes stop web_ui_guest || true
      devenv processes stop web_ui_admin || true
    '';

    # Launch full application stack (APIs + Topcoat frontends, DB kept running continuously)
    fullstack.exec = ''
      echo "Starting full development stack (APIs, Frontends, Pub/Sub and Cloud Storage emulators, database)..."
      db-start
      pubsub-start
      gcs-start
      devenv up listing_api booking_api user_api email_worker image_worker web_ui_guest web_ui_admin "$@"
    '';

    # Services status inspection
    services-status.exec = ''
      "$DEVENV_ROOT/scripts/services_status.sh" "$@"
    '';

    # Playwright E2E Testing
    playwright-install.exec = ''
      [ -d "node_modules" ] || npm ci
      echo "Installing Playwright browsers (chromium, firefox)..."
      npx playwright install --with-deps chromium firefox
      for d in "$HOME"/.cache/ms-playwright/firefox-*; do
        [ -d "$d" ] && touch "$d/DEPENDENCIES_VALIDATED"
      done
    '';

    test-e2e.exec = ''
      set -e
      [ -d "node_modules" ] || npm ci
      echo "Running Playwright E2E test suites..."
      npx playwright test --config=playwright/playwright.config.ts "$@"
    '';

    test-e2e-guest.exec = ''
      set -e
      [ -d "node_modules" ] || npm ci
      echo "Running Guest Portal Playwright E2E tests (Chromium & Firefox)..."
      npx playwright test --config=playwright/playwright.config.ts --project=guest-portal-chromium --project=guest-portal-firefox "$@"
    '';

    test-e2e-admin.exec = ''
      set -e
      [ -d "node_modules" ] || npm ci
      echo "Running Admin Portal Playwright E2E tests (Chromium & Firefox)..."
      npx playwright test --config=playwright/playwright.config.ts --project=admin-portal-chromium --project=admin-portal-firefox "$@"
    '';

    test-e2e-ui.exec = ''
      set -e
      [ -d "node_modules" ] || npm ci
      echo "Opening Playwright Interactive UI Mode..."
      npx playwright test --config=playwright/playwright.config.ts --ui "$@"
    '';

    test-e2e-chromium.exec = ''
      set -e
      [ -d "node_modules" ] || npm ci
      echo "Running Playwright E2E tests (Chromium only across Guest & Admin)..."
      npx playwright test --config=playwright/playwright.config.ts --project=guest-portal-chromium --project=admin-portal-chromium "$@"
    '';

    test-e2e-ui-chromium.exec = ''
      set -e
      [ -d "node_modules" ] || npm ci
      echo "Opening Playwright Interactive UI Mode (Chromium only across Guest & Admin)..."
      npx playwright test --config=playwright/playwright.config.ts --ui --project=guest-portal-chromium --project=admin-portal-chromium "$@"
    '';
  };

  # Process manager configuration (run via `devenv up` or `apis`)
  processes = {

    # Listing API (Port 8082)
    listing_api.exec = ''
      cd app_api/listing_api && ./run_local.sh
    '';

    # Booking API (Port 8081)
    booking_api.exec = ''
      cd app_api/booking_api && ./run_local.sh
    '';

    # User API (Port 8083)
    user_api.exec = ''
      cd app_api/user_api && ./run_local.sh
    '';

    # Email Worker (Port 8080)
    email_worker.exec = ''
      cd app_api/email_worker && ./run_local.sh
    '';

    # Image Worker (Port 8084): resizes photos pushed from the raw bucket
    image_worker.exec = ''
      cd app_api/image_worker && ./run_local.sh
    '';


    # Guest Portal (Topcoat UI). It refuses to start without JWT_SECRET;
    # local backends fall back to "secret", so match that.
    web_ui_guest.exec = ''
      cd web_ui_guest && PORT=3010 JWT_SECRET="''${JWT_SECRET:-secret}" CARGO_TARGET_DIR=../target/ui_guest topcoat dev
    '';

    # Admin Portal (Topcoat UI)
    web_ui_admin.exec = ''
      cd web_ui_admin && PORT=3012 JWT_SECRET="''${JWT_SECRET:-secret}" CARGO_TARGET_DIR=../target/ui_admin topcoat dev
    '';
  };

  enterShell = ''
    echo "🚀 Welcome to our_places_rs dev environment!"
    echo "   - Rust:       $(rustc --version)"
    echo "   - Cargo:      $(cargo --version)"
    echo "   - Node:       $(node --version)"
    echo "   - Python:     $(python3 --version)"
    echo "   - Java:       $(java -version 2>&1 | head -n 1)"
    echo "   - PostgreSQL: $(psql --version)"
    echo "   - sqlx-cli:   $(sqlx --version)"
    echo "   - Workflows & Launchers (Foreground - Ctrl+C to stop):"
    echo "       • apis           (launch DB + Pub/Sub + microservices):"
    echo "           - email_worker:     :8080"
    echo "           - booking_api:      :8081"
    echo "           - listing_api:      :8082"
    echo "           - user_api:         :8083"
    echo "           - image_worker:     :8084"
    echo "           - pubsub_emulator:  :8085"
    echo "           - gcs_emulator:     :4443"
    echo "       • apis-start     (launch DB + Pub/Sub + APIs in background / detached)"
    echo "       • apis-stop      (stop background APIs & Pub/Sub)"
    echo "       • pubsub-start   (start Pub/Sub emulator + init topic & push subscription)"
    echo "       • pubsub-stop    (stop Pub/Sub emulator)"
    echo "       • gcs-start      (start Cloud Storage emulator + create photo buckets)"
    echo "       • gcs-stop       (stop Cloud Storage emulator)"
    echo "       • frontends      (launch Topcoat UI portals):"
    echo "           - web_ui_guest:     :3010 (guest)"
    echo "           - web_ui_admin:     :3012 (admin)"
    echo "       • frontends-stop (stop portals)"
    echo "       • fullstack      (launch full stack: DB + APIs + Frontends)"
    echo "       • services-status(inspect status of APIs, Frontends, Database, and Pub/Sub)"
    echo "       • test-e2e       (run Playwright end-to-end tests across Chromium & Firefox)"
    echo "       • test-e2e-ui-chromium (launch interactive UI for Chromium across Guest & Admin)"
    echo "       • db-start       • db-stop        • db-seed        • db-migrate     • db-prepare"
    echo "           - postgres:         :5432"
    echo "       • docker-db-start• docker-db-stop"
    echo "       • check-all      • sanity-check   • test-ci-matrix • audit-booking"
    echo "       • security-audit • eval-skills"
  '';
}
