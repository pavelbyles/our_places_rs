---
name: sonarqube-guard
description: Implementation guide for satisfying SonarQube, SonarCloud, and DeepSource static analysis standards across Rust code, email templates, and database migrations.
version: 2.0.0
rpi_phase: Implementation
trigger:
  - Generate Rust struct, enum, function, or module
  - Create or edit HTML email templates
  - Read environment variables with env::var
  - Modify database migrations or SQL files
  - Resolve SonarQube, SonarCloud, or DeepSource static analysis issues
capabilities:
  - Enforce doc coverage on public types, fields, and variants
  - Disallow raw string literals in env::var calls
  - Enforce std::env::temp_dir for temporary files
  - Standardize HTML template headers and accessibility contrast
  - Guard database migration immutability and exclusions
  - Verify panic-free zero-unwrap error handling
---

# SonarQube & DeepSource Hygiene Guard

> **Authority**: [CONSTRAINTS.md](../../../CONSTRAINTS.md) defines the non-negotiable project quality contract and pass/fail thresholds. This skill provides the code transformations, structural patterns, and implementation guidance required to meet those static analysis gates.

---

## Core Transformation Patterns

### 1. Full Doc Coverage (`RS-D1001`, `missing_docs`)
- Every public `struct`, `enum`, `fn`, `trait`, and `mod` MUST have a `///` doc comment.
- Every **public struct field** and **enum variant** MUST have an individual `///` doc comment.
- Module declarations in `lib.rs` / `mod.rs` MUST be documented.

```rust
/// Represents a guest booking hold status.
pub enum HoldStatus {
    /// Hold has been reserved and is pending payment confirmation.
    Pending,
    /// Hold was confirmed following a successful payment capture.
    Confirmed,
}
```

### 2. Strongly-Typed Environment Access (`RS-W1015`)
- NEVER pass raw string literals into `std::env::var("KEY")`.
- Declare a static constant or encapsulate configuration in a dedicated struct:

```rust
// BAD
let secret = std::env::var("JWT_SECRET")?;

// GOOD
static JWT_SECRET_ENV: &str = "JWT_SECRET";
let secret = std::env::var(JWT_SECRET_ENV)?;
```

### 3. Cross-Platform Temporary Directories (`RS-S1003`)
- NEVER hardcode `"/tmp"` or platform-specific paths.
- Always invoke `std::env::temp_dir()`:

```rust
// BAD
let path = std::path::Path::new("/tmp/upload.jpg");

// GOOD
let path = std::env::temp_dir().join("upload.jpg");
```

### 4. Idiomatic Initializers (`RS-W1079`)
- Prefer `String::default()` over `String::new()` in mapping closures and default expressions.

### 5. HTML Email Template Standards (Sonar S5254, S5148, WCAG AA)
- Must declare `<!DOCTYPE html>` and `<html lang="en">`.
- Must include `<head><meta charset="utf-8"><title>Descriptive Title - Our Places</title></head>`.
- Interactive elements and text must satisfy WCAG 2.1 AA contrast ratios ($\ge 4.5:1$).

### 6. Database Migration Immutability
- NEVER edit applied migration scripts in `db_core/migrations/` (modifying invalidates SQLx checksums).
- Verify that `**/db_core/migrations/**` remains excluded in `.sonarcloud.properties`, `sonar-project.properties`, and `.deepsource.toml`.

### 7. Zero-Panic Error Propagation (`RS-W1072`)
- NEVER use `.unwrap()` or `.expect()` in production code paths.
- Propagate errors using `?`, `.unwrap_or_default()`, or monadic combinators (`.and_then()`, `.map()`).
- Suppressions (`// skipcq:`) lower the bar and MUST be logged in [CONSTRAINTS.md](../../../CONSTRAINTS.md#4-exceptions-log).

---

## Pre-Commit Verification Checklist

- [ ] All new or modified public structs, fields, enums, variants, and functions have `///` doc comments
- [ ] No raw string literals in `env::var(...)` calls
- [ ] No hardcoded `/tmp` paths; uses `std::env::temp_dir()`
- [ ] HTML email templates include `lang="en"`, `<head>`, `<meta charset="utf-8">`, and `<title>`
- [ ] Zero `.unwrap()` / `.expect()` in production code paths
- [ ] No modifications to previously applied database migrations
- [ ] Any necessary suppressions (`// skipcq`) are recorded in [CONSTRAINTS.md](../../../CONSTRAINTS.md#4-exceptions-log)

---

## Detailed References & Before/After Catalog
* For code transformations, suppression tags, and exclusion configurations, see **[REFERENCE.md](REFERENCE.md)**.
