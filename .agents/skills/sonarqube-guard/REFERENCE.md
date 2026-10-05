# SonarQube & DeepSource Hygiene Reference Guide

This reference provides actionable code patterns, issue code mappings, and configuration blueprints to ensure all generated code passes SonarQube, SonarCloud, DeepSource, and Clippy quality gates cleanly on the first attempt.

---

## 1. Rule Catalog & Issue Code Mapping

| Analyzer | Rule ID | Category | Description | Requirement |
| :--- | :--- | :--- | :--- | :--- |
| **DeepSource** | `RS-D1001` | Documentation | Undocumented public struct, enum, function, field, or variant | Every public item, including struct fields and enum variants, must have a `///` doc comment. |
| **DeepSource** | `RS-W1015` | Bug Risk | String literal used directly in `env` functions | Declare a `static ..._ENV: &str = "...";` constant or use structured configuration. |
| **DeepSource** | `RS-S1003` | Security | Hardcoded temporary file directory (`/tmp`) | Always use `std::env::temp_dir().join(...)`. |
| **DeepSource** | `RS-W1079` | Style | Empty call to `new()` where `Default` is idiomatic | Use `String::default()` or `Default::default()`. |
| **DeepSource** | `RS-R1000` | Complexity | High cognitive / cyclomatic complexity | Refactor into monadic combinators or annotate with `// skipcq: RS-R1000`. |
| **DeepSource** | `RS-W1072` | Reliability | Use of `.unwrap()` or `.expect()` in production | Always use `?`, `.unwrap_or_default()`, or monadic combinator pipelines. |
| **SonarCloud** | `Web:S5254` | Accessibility | Missing `lang` attribute on `<html>` tag | Always set `<html lang="en">` on all HTML templates. |
| **SonarCloud** | `Web:S5148` | Standards | Missing `<head>` and `<title>` elements | Every HTML document must contain `<head><meta charset="utf-8"><title>...</title></head>`. |
| **SonarCloud** | `Web:S5256` | Accessibility | Low contrast ratio between text and background | Interactive elements must satisfy WCAG AA contrast (e.g., `#0369a1` on white). |
| **SonarCloud** | `Plsql:S*` | Portability | SQL dialect parser errors on migrations | Exclude `db_core/migrations/**` in Sonar properties; **never** alter applied migrations. |

---

## 2. Before & After Code Transformations

### A. Full Documentation Coverage (`RS-D1001`)

DeepSource and SonarQube enforce doc comments on **every public item**. For structs and enums, documenting only the type itself is **not enough**—every public field and every enum variant must also have a doc comment.

#### ❌ Non-Compliant:
```rust
// Missing field and variant docs
pub enum BookingStatus {
    Pending,
    Confirmed,
    Cancelled,
}

pub struct NewBookingRequest {
    pub guest_id: Uuid,
    pub listing_id: Uuid,
    pub total_price: Decimal,
}
```

#### ✅ Compliant:
```rust
/// Lifecycle status for reservations and bookings.
#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq)]
pub enum BookingStatus {
    /// Initial payment pending state (15-minute hold).
    Pending,
    /// Payment confirmed and reservation secured.
    Confirmed,
    /// Reservation cancelled.
    Cancelled,
}

/// Request payload for creating a new booking reservation.
#[derive(Debug, Deserialize, Serialize, Validate)]
pub struct NewBookingRequest {
    /// Guest user identifier.
    pub guest_id: Uuid,
    /// Property listing identifier.
    pub listing_id: Uuid,
    /// Total price charged for reservation.
    pub total_price: Decimal,
}
```

#### Module Documentation in `lib.rs` / `mod.rs`:
```rust
/// Booking domain entities and operations.
pub mod booking;
/// Guest-to-host messaging pipelines.
pub mod booking_message;
```

---

### B. Environment Variable Functions (`RS-W1015`)

Passing string literals directly into `std::env::var(...)` is flagged by DeepSource.

#### ❌ Non-Compliant:
```rust
let db_url = std::env::var("DATABASE_URL")
    .unwrap_or_else(|_| "postgres://localhost:5432/db".to_string());
let host = std::env::var("SMTP_HOST").unwrap_or_else(|_| "localhost".to_string());
```

#### ✅ Compliant:
```rust
static DATABASE_URL_ENV: &str = "DATABASE_URL";
static SMTP_HOST_ENV: &str = "SMTP_HOST";

let db_url = std::env::var(DATABASE_URL_ENV)
    .unwrap_or_else(|_| "postgres://localhost:5432/db".to_string());
let host = std::env::var(SMTP_HOST_ENV).unwrap_or_else(|_| "localhost".to_string());
```

---

### C. Temporary Directory Hygiene (`RS-S1003`)

Never hardcode Unix-specific `/tmp` paths; use platform-agnostic `std::env::temp_dir()`.

#### ❌ Non-Compliant:
```rust
let email_dir = std::path::Path::new("/tmp/our_places_emails");
let file_path = format!("/tmp/preview_{}.html", id);
```

#### ✅ Compliant:
```rust
let email_dir = std::env::temp_dir().join("our_places_emails");
let _ = std::fs::create_dir_all(&email_dir);
let file_path = email_dir.join(format!("preview_{}.html", id));
```

---

### D. Empty Call to `new()` (`RS-W1079`)

In closures or default initializations, prefer `String::default()` or `Default::default()`.

#### ❌ Non-Compliant:
```rust
serde_json::Value::Null => String::new(),
```

#### ✅ Compliant:
```rust
serde_json::Value::Null => String::default(),
```

---

### E. Standards-Compliant HTML Templates (SonarCloud S5254, S5148, WCAG AA)

All transactional HTML templates in `app_api/email_worker/templates/` must contain valid document structure and WCAG AA contrast colors.

#### ❌ Non-Compliant:
```html
<!DOCTYPE html>
<html>
<body>
  <h2>Notification</h2>
  <a href="{{url}}" style="background-color: #0284c7; color: #fff;">Click Here</a>
</body>
</html>
```

#### ✅ Compliant:
```html
<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <title>Notification - Our Places</title>
</head>
<body style="font-family: Arial, sans-serif; background-color: #f8fafc; color: #1e293b; padding: 24px;">
  <h2>Notification</h2>
  <div style="text-align: center; margin: 32px 0;">
    <!-- #0369a1 satisfies WCAG 2.1 AA 4.5:1 contrast against white text -->
    <a href="{{url}}" style="background-color: #0369a1; color: #ffffff; text-decoration: none; padding: 14px 28px; border-radius: 6px; font-weight: bold; font-size: 16px; display: inline-block;">Click Here</a>
  </div>
</body>
</html>
```

---

### F. Database Migration Immutability & Static Analysis Exclusions

#### Hard Rule: Never Edit Applied Migrations
Altering an existing file under `db_core/migrations/` invalidates the SQLx migration checksum in preview and production environments:
```
error: migration 20260927000000_... was previously applied but has been modified
```
If SonarQube or DeepSource flags syntax errors (such as `pg_cron` procedural blocks or dollar quotes `$cron$`), **DO NOT edit the migration**. Instead, ensure static analysis exclusions are configured.

#### Required `.sonarcloud.properties` and `sonar-project.properties`:
```properties
sonar.projectKey=pavelbyles_our_places_rs
sonar.organization=pavelbyles
sonar.exclusions=**/db_core/migrations/**,**/node_modules/**,**/target/**
```

#### Required `.deepsource.toml`:
```toml
version = 1

test_patterns = [
  "**/tests/**",
  "**/tests.rs",
  "**/test_*.rs"
]

exclude_patterns = [
  "db_core/migrations/**",
  "target/**",
  "node_modules/**"
]

[[analyzers]]
name = "rust"
enabled = true

  [analyzers.meta]
  msrv = "stable"
```

---

### G. Zero-Panic Error Handling & Complexity Suppressions

In accordance with `AGENTS.md` and DeepSource `RS-W1072`, never call `.unwrap()` or `.expect()` in non-test production code.

#### ❌ Non-Compliant:
```rust
let id_uuid = uuid::Uuid::parse_str(id_str).unwrap();
let token = generate_jwt().unwrap();
```

#### ✅ Compliant:
```rust
// Using if-let pattern match
if let Ok(id_uuid) = uuid::Uuid::parse_str(id_str) {
    ...
}

// Or using fallback or monadic error propagation
let token = generate_jwt().unwrap_or_default();
let id_uuid = uuid::Uuid::parse_str(id_str).map_err(|_| AppError::BadRequest("Invalid UUID".into()))?;
```

#### Complexity Suppression Annotation:
When a database orchestration function has high cyclomatic complexity that cannot be decomposed without harming transactional coherence:
```rust
/// Updates a booking's status and executes transactional transition side-effects.
// skipcq: RS-R1000
#[tracing::instrument(skip(pool))]
pub async fn update_booking(
    pool: &PgPool,
    ...
) -> Result<Booking, AppError> {
    ...
}
```
