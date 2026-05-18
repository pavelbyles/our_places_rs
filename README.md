# our_places_rs

`our_places_rs` is a monorepo containing a full-stack Rust application for a villa booking system.

## Workspace Structure

- **web_app/**: Leptos-based frontend (Wasm).
- **web_app_admin/**: Leptos-based administrative frontend (Wasm).
- **app_api/**: Backend services (Axum).
    - `api_core`: Core utilities for the APIs.
    - `booking_api`: Handles reservations.
    - `listing_api`: Manages property listings.
    - `user_api`: User authentication and management.
    - `image_worker`: Background worker for image processing.
- **common/**: Shared business logic and types.
- **db_core/**: Database entities and SQLx interaction.
- **protoproj/**: Protocol Buffers definitions.
- **infra/**: Infrastructure as Code (Terraform/Pulumi).

## Core Workflows

### Running everything
We use Docker Compose to orchestrate local development:
```bash
docker compose up -d
```
This spins up Postgres and potentially the services depending on the configuration.

### Building
Build the entire workspace:
```bash
cargo build --workspace
```

### Testing
Run tests across all crates:
```bash
cargo test --workspace
```

## Shared Rules
- All shared types live in `db_core` or `common`.
- No circular dependencies between sibling crates.
- Use `cargo clippy` to ensure code quality.

## Infrastructure Commands

```bash
gcloud dns --project=our-places-dev managed-zones create ourplaces-dev-api-zone --description="" --dns-name="api.dev.ourplaces.io." --visibility="private" --networks="default"

# Generate certificate for API's
gcloud compute ssl-certificates create ourplaces-apicertdev \
    --description="Certificate for dev apis" \
    --domains=dev.api.ourplaces.io \
    --global

# TF version
resource "google_compute_managed_ssl_certificate" "lb_default" {
  provider = google-beta
  name     = "ourplaces-apicertdev"

  managed {
    domains = [dev.api.ourplaces.io]
  }
}

# List certs
gcloud compute ssl-certificates list \
   --global
```
