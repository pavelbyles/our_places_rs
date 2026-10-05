---
name: sonarqube-guard
description: Enforce SonarQube, SonarCloud, and DeepSource static analysis standards across Rust code, email templates, and database migrations.
version: 1.0.0
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
tools:
  - name: verify hygiene
    description: Scan workspace for SonarQube, DeepSource, and clippy hygiene violations
    entrypoint: scripts/verify_sonarqube_hygiene.sh
---

# SonarQube & DeepSource Hygiene Guard

Enforce strict static analysis hygiene for SonarQube, SonarCloud, and DeepSource across the workspace.

## Core Rules for Code Generation

1. **Full Doc Coverage (`RS-D1001`)**:
   - Every public struct, enum, function, method, and module MUST have a `///` doc comment.
   - Every **public struct field** and **enum variant** MUST have an individual `///` doc comment.
   - Module declarations in `lib.rs` / `mod.rs` MUST be documented.

2. **No String Literals in `env::var` (`RS-W1015`)**:
   - NEVER pass string literals to `std::env::var("KEY")`.
   - Declare a `static KEY_ENV: &str = "KEY";` constant or use strongly-typed config structs.

3. **Temporary Directories (`RS-S1003`)**:
   - NEVER hardcode `"/tmp"` or `"/tmp/..."`. Always use `std::env::temp_dir().join(...)`.

4. **Empty Initializers (`RS-W1079`)**:
   - Use `String::default()` instead of `String::new()` in mapping closures and default expressions.

5. **HTML Email Templates (Sonar S5254, S5148, WCAG AA)**:
   - Must declare `<!DOCTYPE html>` and `<html lang="en">`.
   - Must include `<head><meta charset="utf-8"><title>Descriptive Title - Our Places</title></head>`.
   - Action buttons and text must meet WCAG 2.1 AA contrast ratios (e.g. `#0369a1` on white).

6. **Migration Immutability & Exclusions**:
   - NEVER edit applied migration files in `db_core/migrations/` (modifying invalidates SQLx checksum).
   - Ensure `**/db_core/migrations/**` is excluded in `.sonarcloud.properties`, `sonar-project.properties`, and `.deepsource.toml`.

7. **Zero-Panic & Monadic Error Handling (`RS-W1072`, `AGENTS.md`)**:
   - NEVER use `.unwrap()` or `.expect()` in production code. Use `?`, `.unwrap_or_default()`, or `if let Ok(...)`.
   - Annotate necessary complex orchestrations with `// skipcq: RS-R1000`.

## Pre-Commit Verification Checklist

- [ ] All new/modified public structs, fields, enums, variants, and functions have `///` doc comments.
- [ ] No raw string literals in `env::var(...)` calls.
- [ ] No hardcoded `/tmp` paths; uses `std::env::temp_dir()`.
- [ ] HTML templates include `lang="en"`, `<head>`, `<meta charset="utf-8">`, and `<title>`.
- [ ] Zero `.unwrap()` / `.expect()` in production code paths.
- [ ] No alterations to previously applied database migrations.

---

## Detailed References & Before/After Catalog
* For code transformations, suppression tags, and exclusion configurations, see **[REFERENCE.md](REFERENCE.md)**.
