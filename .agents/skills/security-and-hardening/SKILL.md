---
name: security-and-hardening
aliases:
  - security-review
  - vulnerability-analysis
description: Audit code and architectures for security vulnerabilities, enforce OWASP Top 10 hardening, evaluate exploitability/reachability, and establish threat models.
version: 2.0.0
---

# Security and Hardening

## Overview

Security is a foundational constraint across every line of code that handles external input, authentication, data persistence, or external communication. Treat every external input as hostile, every secret as sacred, and every authorization check as mandatory.

This skill unifies proactive threat modeling, code-level hardening against the OWASP Top 10, and vulnerability reachability analysis for dependencies and scanner alerts.

---

## When to Use

- Building or modifying endpoints that accept user input or file uploads.
- Implementing or altering authentication, session management, or authorization logic.
- Storing, transmitting, or transforming sensitive credentials, PII, or financial data.
- Integrating external APIs, third-party webhooks, or background processing pipelines.
- Triaging dependency vulnerability scanners (`cargo audit`, `npm audit`, Dependabot, Snyk).
- Reviewing PR diffs for security posture before release.

### When NOT to Use
- Mechanical code cleanup (variable renaming, formatting, non-security refactoring).
- Pure internal unit testing that does not mock or cross security boundaries.
- General compiler/linter error resolution (use language-specific linters).

---

## The 5-Step Security Audit & Hardening Protocol

Execute these steps systematically:

```
1. SCOPE & BOUNDARIES ──→ 2. THREAT MODEL (STRIDE) ──→ 3. THREE-TIER HARDENING
                                                               │
5. FINDINGS & FIXES    ←── 4. REACHABILITY & CVSS     ←────────┘
```

### Step 1: Scope & Map Trust Boundaries
Identify where untrusted data crosses into the system:
- **Trust Boundaries**: HTTP request parameters, headers, cookies, file uploads, webhooks, message queues, external database results, and LLM outputs.
  > *Rule*: Trust follows who *wrote* the value, not the channel that delivered it. An environment variable or command-line argument passed from another process is as untrusted as an HTTP query string.
- **Identify Assets**: Credentials, session tokens, personal identity information (PII), billing/payment data, administrative capabilities, and system state.

### Step 2: STRIDE Threat Modeling & Abuse Cases
Run a STRIDE analysis over each boundary and write abuse cases alongside use cases:

| Threat | Key Question | Standard Mitigation |
| :--- | :--- | :--- |
| **S**poofing | Can an attacker impersonate a user or service? | Mutual TLS, cryptographic signatures, strict JWT verification |
| **T**ampering | Can payload data or state be altered in transit/at rest? | HTTPS, message integrity checks, parameterized queries |
| **R**epudiation | Can a critical action be denied after the fact? | Immutable audit logging of security-critical state transitions |
| **I**nformation Disclosure | Can sensitive internals or user data leak? | Encryption at rest, allowlisted JSON serialization, sanitized errors |
| **D**enial of Service | Can resource consumption be forced to exhaustion? | Rate limiting, request payload size limits, strict timeouts |
| **E**levation of Privilege | Can an unprivileged caller perform admin actions? | Role-based / claims-based authorization, explicit ownership checks |

*Write the Abuse Case*: For every feature, define: *"How would a malicious actor deliberately misuse this endpoint or input?"* Write a test that attempts this abuse.

### Step 3: Enforce the Three-Tier Boundary System

#### Tier 1: Always Do (Zero Exceptions)
- **Parameterize all database queries**: Never format or concatenate raw strings into SQL/NoSQL queries.
- **Validate external input at the boundary**: Enforce strict allowlists (lengths, schemas, regexes, ranges). Reject invalid data early with HTTP 422/400.
- **Check resource authorization on every access (IDOR Prevention)**: Do not rely solely on authentication. Verify that the authenticated user explicitly owns or is authorized to read/modify the specific target entity ID.
- **Hash passwords securely**: Use modern adaptive hashing (Argon2id, bcrypt $\ge 12$ rounds, or scrypt). Never store or log plaintext passwords.
- **Set defensive cookie and transport flags**: Cookies carrying session identifiers must be `HttpOnly`, `Secure`, and `SameSite=Lax` or `Strict`. Enforce HTTPS/TLS everywhere.
- **Sanitize error outputs**: Return generic error descriptions to API clients. Log internal stack traces, database errors, and system details server-side only.

#### Tier 2: Ask First (Requires Explicit Confirmation)
- Modifying authentication workflows, token signing schemes, or password hashing configurations.
- Introducing new categories of sensitive data (PII, financial instruments, biometric data).
- Modifying CORS policies to allow new external origins or wildcard permissions.
- Creating file upload endpoints or webhooks without pre-existing sandboxing.
- Relaxing rate-limiting thresholds or granting elevated system privileges.

#### Tier 3: Never Do
- **Never commit secrets**: No API keys, private keys, database passwords, or JWT secrets in version control. Staged diffs must be checked before commit.
- **Never log sensitive data**: Mask or redact passwords, bearer tokens, credit card numbers, and PII from application logs.
- **Never trust client-side validation alone**: Client checks improve UX; server-side boundary validation enforces security.
- **Never use dynamic code execution (`eval`, raw shell commands)** on untrusted input strings.
- **Never disable security headers or HTTPS** for local convenience.

---

### Step 4: Vulnerability Triage & Reachability Analysis

When triaging scanner alerts (`cargo audit`, `npm audit`, Dependabot, Snyk):
1. **Verify Reachability**:
   - Is the vulnerable function or module actually imported and called in the execution graph?
   - Can untrusted external input reach the arguments of the vulnerable dependency?
   - If the vulnerability exists in a dev dependency (e.g., test runner) and is unreachable in production artifacts, document the exclusion with reasoning.
2. **Assign SLA Priority Tiers**:
   - **P0 (Critical / High Reachable)**: Actively exploitable from external interfaces or network boundaries. **Remediate within 24 hours**.
   - **P1 (Medium Reachable / High Internal)**: Reachable only behind authenticated boundaries or with high operational complexity. **Remediate in current release cycle**.
   - **P2 (Low / Unreachable / Dev-only)**: Defense-in-depth or unreachable code path. **Schedule for normal dependency maintenance**.

---

### Step 5: Document Findings & Remediate

Report all findings using the standard findings schema:

```markdown
### Security Finding: [Short Descriptive Title]
- **Severity**: Critical | High | Medium | Low
- **Category**: [e.g., A01: Broken Access Control (IDOR) | A03: Injection | A10: SSRF]
- **Location**: `path/to/file.ext:L123`
- **Impact**: [Concrete risk — what unauthorized action or data disclosure can occur]
- **Vulnerability Explanation**: [Root cause explanation of why the check fails]
- **Remediation**:
```diff
- // Vulnerable code snippet
+ // Hardened, safe replacement
```
```

---

## Detailed Hardening Controls

### 1. SSRF (Server-Side Request Forgery) Prevention
When fetching URLs provided by or derived from user input:
- Validate schemes strictly (allowlist `https://` only; reject `file://`, `gopher://`, `ftp://`).
- Resolve all DNS records before making the request. Reject any address resolving to loopback (`127.0.0.0/8`, `::1`), link-local (`169.254.0.0/16`), private ranges (`10.0.0.0/8`, `172.16.0.0/12`, `192.168.0.0/16`), or multicast.
- Disable automatic HTTP redirects, or re-validate redirect destinations against the IP filter before following.

### 2. Destructive Operations on Derived Paths
When deleting, moving, or modifying files based on input:
- Resolve all symlinks first (`realpath` / canonicalize).
- Verify the canonical path resides under a strictly allowlisted base directory.
- Enforce a minimum directory depth so the root cannot be selected.
- Validate resource ownership before performing the destructive operation.

### 3. File Upload Hardening
- Do not trust client-supplied file extensions or MIME types.
- Validate magic bytes / file signatures on upload.
- Cap maximum payload sizes before buffering.
- Store uploaded files in an isolated object store or outside the executable document root with randomized filenames.

### 4. Rate Limiting
- Enforce strict rate limits on authentication, password reset, and sensitive endpoints (e.g., $\le 10$ attempts per 15 minutes).
- Back rate limiters with a shared persistence store when running across multiple container instances to avoid distributed bypass.

---

## Anti-Rationalization Table

| Rationalization | Reality |
| :--- | :--- |
| *"This endpoint is internal, so we don't need authorization checks."* | Perimeter defenses fail. Defense-in-depth requires explicit authorization on every service boundary. |
| *"The client framework auto-escapes, so input validation is redundant."* | Auto-escaping prevents XSS; it does not stop SQL injection, business logic abuse, buffer overruns, or SSRF. |
| *"We can patch the vulnerability later after launch."* | Vulnerabilities are discovered by automated scanners minutes after deployment. Security is a release gate. |
| *"The scanner reported a CVE, so we must immediately bump and push --force."* | Forced bumps can break transitive contracts. Triage reachability first; verify the update passes all tests before merging. |
| *"It's just an error message; showing the query helps debugging."* | Leaking database schema or stack traces gives attackers precise blueprints for injection. Keep details in server logs. |

---

## Verification Checklist

- [ ] All external input boundaries enforce strict schema validation
- [ ] Database interactions use compile-time or parameterized queries (zero string formatting)
- [ ] IDOR checks enforce resource ownership on every protected entity ID
- [ ] Passwords hashed with adaptive functions (Argon2id / bcrypt $\ge 12$ rounds)
- [ ] Session tokens and cookies are marked `HttpOnly`, `Secure`, and `SameSite`
- [ ] Generic error responses returned to clients; internal details restricted to server logs
- [ ] Staged git diff inspected for plaintext secrets or key material
- [ ] Native dependency audit executed clean with zero unresolved P0/P1 reachability alerts
- [ ] Abuse cases and regression tests accompany every security-critical feature
