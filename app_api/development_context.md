# App API Development Context

## Overview
The `app_api` directory contains the backend microservices built with **Axum**.

## Services
1.  **listing_api**: Manages property listings (CRUD).
2.  **booking_api**: Handles booking logic and reservation atomicity.
3.  **user_api**: Authentication (JWT) and user profiles.
4.  **image_worker_api**: Handles image processing and storage.

## Shared Dependencies
- **db_core**: All services depend on `db_core` for database access and shared types.
- **common**: Shared utilities and types.

## Database
- Uses **PostgreSQL**.
- **SQLx** is used for compile-time checked queries.
- Migrations are managed via `sqlx-cli`.

## running a Service
Navigate to a service directory and run:
```bash
cargo run
```
Ensure `DATABASE_URL` and other env vars are set (usually via `.env`).

## API Structure
- **Clean Architecture**: Handlers -> Services -> Repositories (in `db_core`).
- **Tracing**: All request handlers should be instrumented.
- **Error Handling**: Use `Result<impl IntoResponse, AppError>`.
