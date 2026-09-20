{ pkgs, ... }:

{
  # Automatically load .env (DATABASE_URL, JWT_SECRET, etc.)
  dotenv.enable = true;

  # Rust Toolchain (rustc 1.98+, cargo, clippy, rustfmt, rust-analyzer)
  languages.rust.enable = true;

  # JavaScript/Node.js Toolchain (node & npm for DaisyUI / Tailwind)
  languages.javascript = {
    enable = true;
    npm.enable = true;
  };

  # Python Toolchain (for .agents/evals/eval_runner.py)
  languages.python.enable = true;

  # Native dependencies & developer CLI utilities
  packages = [
    pkgs.sqlx-cli
    pkgs.cargo-watch
    pkgs.cargo-audit
    pkgs.ripgrep
    pkgs.pkg-config
    pkgs.openssl
    pkgs.postgresql
  ];

  # Fast git pre-commit formatting check
  git-hooks.hooks = {
    rustfmt.enable = true;
  };

  # Project workflow commands mirroring .agents/ workflows
  scripts = {
    # Database migrations & metadata
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
      cargo test --workspace --exclude web_app_tc --exclude web_app_admin_tc --exclude web_app_common_tc --exclude protoproj
      echo "--> Running Topcoat Web & Common Tests (mirrors test-web-tc)..."
      cargo test -p web_app_tc -p web_app_common_tc
      echo "--> Running Topcoat Admin Tests (mirrors test-web-admin-tc)..."
      cargo test -p web_app_admin_tc
    '';

    # Float-Ban & Booking Logic Audit (.agents/workflows/audit-booking-flow.md)
    audit-booking.exec = ''
      echo "Checking for illegal floating-point types (f32/f64) in pricing & booking..."
      if rg --type rust "f32|f64" common/ app_api/booking_api/ db_core/; then
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

    # Launch all API microservices and database
    apis.exec = ''
      echo "Starting database and API microservices (listing_api, booking_api, user_api)..."
      devenv up db listing_api booking_api user_api "$@"
    '';
  };

  # Process manager configuration (run via `devenv up` or `apis`)
  processes = {
    # Database watcher & log streamer (ensures Docker container is active and accepts connections)
    db.exec = ''
      if ! pg_isready -h localhost -p 5432 >/dev/null 2>&1; then
        echo "Starting Docker container 'ourplaces_db'..."
        docker start ourplaces_db 2>/dev/null || docker compose up -d db
      fi
      until pg_isready -h localhost -p 5432 >/dev/null 2>&1; do
        sleep 1
      done
      echo "PostgreSQL is accepting connections on localhost:5432"
      docker logs -f ourplaces_db
    '';

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

    # Guest Portal Frontend (Topcoat SSR & HTMX)
    web_app_tc.exec = ''
      cd web_app_tc && topcoat dev
    '';

    # Admin Portal Frontend (Topcoat SSR & HTMX)
    web_app_admin_tc.exec = ''
      cd web_app_admin_tc && topcoat dev
    '';
  };

  enterShell = ''
    echo "🚀 Welcome to our_places_rs dev environment!"
    echo "   - Rust:     $(rustc --version)"
    echo "   - Cargo:    $(cargo --version)"
    echo "   - Node:     $(node --version)"
    echo "   - Python:   $(python3 --version)"
    echo "   - sqlx-cli: $(sqlx --version)"
    echo "   - Workflows & Launchers:"
    echo "       • apis           (launch DB + listing_api, booking_api, user_api)"
    echo "       • devenv up      (launch full stack: DB + APIs + Frontends)"
    echo "       • db-migrate     • db-prepare     • check-all"
    echo "       • sanity-check   • test-ci-matrix • audit-booking"
    echo "       • security-audit • eval-skills"
  '';
}
