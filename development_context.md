# Root Development Context

## Project Overview
`our_places_rs` is a monorepo containing a full-stack Rust application for a villa booking system.

## Workspace Structure
- **web_app/**: Leptos-based frontend (Wasm).
- **app_api/**: Backend services (Axum).
    - `booking_api`: Handles reservations.
    - `listing_api`: Manages property listings.
    - `user_api`: User authentication and management.
    - `image_worker_api`: Handles image processing and storage.
- **common/**: Shared business logic and types.
- **db_core/**: Database entities and SQLx interaction.
- **infra/**: Infrastructure as Code (Terraform/Pulumi).
- **scripts/**: Utility scripts.

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
