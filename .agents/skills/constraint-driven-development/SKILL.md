---
name: constraint-driven-development
aliases:
  - constraints
  - quality-bar
  - quality-gates
description: Establish a project's quality bar as a written contract in CONSTRAINTS.md with sane defaults, ratchets, and anti-cheating rules.
version: 2.0.0
---

# Constraint-Driven Development

## Overview

Other skills describe what good code looks like in prose. An agent reads prose and may or may not follow it, and none of it survives the end of a session.

Constraint-Driven Development establishes a project's quality bar as a written, machine-checkable contract in `CONSTRAINTS.md`. It defines explicit thresholds, places checks according to computational cost, and prevents agents from taking the path of least resistance (silencing linters, skipping tests, or hollowing out assertions to turn CI green).

Spec-driven development defines *what to build*. Test-driven development proves *it works*. Constraint-driven development defines *what is good enough to merge*.

---

## When to Use

- Starting a new project or significant feature when no quality standards are written down.
- The user requests to "set up constraints", "define our quality bar", "set quality gates", or "prevent agents lowering standards".
- An agent is generating high volumes of code without human line-by-line inspection.
- The team argues about coverage, lint strictness, or performance thresholds on every PR instead of deciding once.
- Setting up autonomous or semi-autonomous development loops where automated quality gates protect main.

### When NOT to Use
- The project already has a populated `CONSTRAINTS.md` and the user is not updating it (read and obey it instead).
- Spikes, throwaway prototypes, or one-off exploratory scripts.
- General code reviews (`pr-analyzer` / `code-review-and-quality`) or CI deployment setup (`ci-cd-and-automation`).

---

## Loading Constraints

The intake interview requires an interactive user. **Do not run the intake interview in non-interactive/autonomous contexts** (CI, `/loop`, headless runners). In unattended contexts where constraints are missing, enforce the Universal Floor below and flag remaining dimensions for human review.

---

## The Process

### Step 1: Detect Before Asking
Never ask what can be read from the workspace. Inspect before asking:
- **Stack & Dependencies**: Read package manifest (`Cargo.toml`, `package.json`, `pyproject.toml`, `go.mod`).
- **Test Runner & Coverage**: Identify configured test runners and existing coverage files.
- **Linters & Formatters**: Check linter configurations (`clippy`, `eslint`, `biome`, `ruff`).
- **CI Pipelines**: Read `.github/workflows/` or `.gitlab-ci.yml`.
- **Guidelines**: Read `AGENTS.md`, `CLAUDE.md`, or `GEMINI.md`.

Report what was detected in 2 lines, then ask only what remains unresolved.

---

### Step 2: The 4-Question Intake Interview
Follow the one-question-at-a-time discipline from `grill-me`. Every question includes a hypothesis (`GUESS:`) and a default answer:

```markdown
Q1: Beyond the universal floor, which quality dimensions do you want mechanically enforced?
    (a) Test coverage on changed code
    (b) Security scanning (code & dependencies)
    (c) Performance budgets (latency or bundle size)
    (d) Accessibility (WCAG 2.1 AA)
    (e) Architectural module boundaries
GUESS: (a) and (b) — you have a test runner configured and accept external inputs.
DEFAULT if unsure: (a) and (b).
```

```markdown
Q2: When an automated check fails mid-task, should the agent block or warn?
GUESS: Block. Unattended agents will ignore warnings that do not halt execution.
DEFAULT if unsure: Block on the floor; warn on other dimensions for the first two weeks.
```

```markdown
Q3: Do you have target threshold numbers in mind, or should I record today's baseline and ratchet (refuse regression)?
GUESS: Record and ratchet. Invented targets get abandoned when existing code fails them immediately.
DEFAULT if unsure: Record and ratchet (see Ratchets below).
```

```markdown
Q4: What is the maximum check execution time you'll tolerate in local task feedback?
GUESS: ~90 seconds. Longer checks cause developers to bypass local verification.
DEFAULT if unsure: 90 seconds locally; unlimited in CI.
```

*Stop at four questions.* A bloated intake creates unmaintainable configurations.

---

### Step 3: Write `CONSTRAINTS.md`
Place `CONSTRAINTS.md` at the repository root. Format:

```markdown
# Quality Constraints

Last reviewed: [YYYY-MM-DD] by @[user]

## 1. Universal Floor (Always Enforced, Zero Exceptions)

- **Zero Suppression Comments**: No new compiler or linter suppressions (e.g., `#[allow(...)]`, `@ts-ignore`, `eslint-disable`, `# noqa`) without an approved exception.
- **Zero Unfinished Stubs**: No `todo!()`, `unimplemented!()`, `throw new Error("Not implemented")`, or empty error catch blocks.
- **Zero Deleted or Silenced Tests**: No test deletions, `#[ignore]`, or `.skip` markers without an explicit rationale in the commit message.
- **Zero Secrets in Code**: No API keys, credentials, or private tokens committed to source.
- **Immutable Gate**: This file must not be weakened to make a failing PR pass.

## 2. Enforced Dimensions

| Dimension | Rule | Checked By | Stage |
| :--- | :--- | :--- | :--- |
| **Types** | Zero type / compiler errors | `<compiler_check>` | Edit loop |
| **Lint** | Zero linter warnings/errors | `<linter_check>` | Edit loop |
| **Secrets** | Zero leaked credentials | `<secrets_scanner>` | Pre-commit |
| **Coverage** | Changed lines $\ge 80\%$ covered | `<coverage_command>` | Task end & CI |
| **Security: Code** | Zero high/critical findings | `<sast_scanner>` | CI |
| **Security: Deps** | Zero unmitigated reachable CVEs | `<dependency_audit>` | Pre-merge & CI |

## 3. Measured Baselines (Ratchets)

| Metric | Current Baseline | Invariant |
| :--- | :--- | :--- |
| **Project Test Coverage** | 72.4% | Must not decrease |
| **Build Artifact Size** | 14.2 MB | Must not increase by >5% |

## 4. Temporary Exceptions

| ID | Rule | Target Path | Justification | Owner | Expiration |
| :--- | :--- | :--- | :--- | :--- | :--- |
| EX-1 | `clippy::too_many_arguments` | `legacy/parser.rs` | Refactor scheduled in PR #104 | @lead | 2026-12-01 |
```

---

### Step 4: Map Each Dimension to Native Commands

Every row in `CONSTRAINTS.md` must name a command that runs today. Never declare an aspiration without an executable command.

| Dimension | Ecosystem | Native Tool | Command | Pass Criteria |
| :--- | :--- | :--- | :--- | :--- |
| **Types / Compile** | Rust | `cargo check` | `cargo check --workspace --all-targets` | Exit 0 |
| | TypeScript | `tsc` | `tsc --noEmit` | Exit 0 |
| | Python | `mypy` | `mypy .` | Exit 0 |
| **Linter** | Rust | `clippy` | `cargo clippy --workspace -- -D warnings` | Zero warnings |
| | TypeScript | `eslint` / `biome` | `eslint .` / `biome check` | Zero warnings |
| | Python | `ruff` | `ruff check .` | Zero warnings |
| **Secrets** | Multi | `gitleaks` | `gitleaks detect --redact --no-banner` | Zero findings |
| **Coverage** | Rust | `cargo-tarpaulin` / `llvm-cov` | `cargo llvm-cov --summary-only` | Changed $\ge 80\%$ |
| | TypeScript | `vitest` / `jest` | `vitest run --coverage` | Changed $\ge 80\%$ |
| | Python | `pytest-cov` | `pytest --cov --cov-report=term` | Changed $\ge 80\%$ |
| **Dependencies** | Rust | `cargo audit` | `cargo audit` | Zero high/crit |
| | Node | `npm audit` | `npm audit --audit-level=high` | Zero high/crit |

---

### Step 5: Wire Checks by Computational Cost

Never run slow checks on every keystroke. Cost determines placement:

```
FAST LOOP (<5s)        ──→ TASK COMPLETION (<90s) ──→ CI / PRE-MERGE (Minutes)
• Types & compilation      • Full unit test suite     • Full integration/E2E
• Staged linters           • Diff coverage check      • SAST & dependency audit
• Secret scan on diff      • Floor guard scan         • Ratchet baseline verification
```

---

### Step 6: The Floor Guard (Anti-Cheating Rules)

Agents seek the shortest path to green. At review time, inspect `git diff` against the merge base for these 5 evasion patterns:

1. **Threshold Lowering**: Modifying `CONSTRAINTS.md` or configuration files to relax coverage, linters, or budgets.
2. **Test Evasion**: Adding `.skip`, `#[ignore]`, deleting test files, or removing assertions from existing tests.
3. **Checker Silencing**: Adding new inline suppression comments (`#[allow(...)]`, `@ts-ignore`, `eslint-disable`, `# noqa`, `nosemgrep`).
4. **Hollow Implementation**: Submitting stubs (`todo!()`, `unimplemented!()`, empty `catch {}` blocks) to bypass compiler checks without delivering behavior.
5. **Unilateral Exceptions**: Adding rows to the Exceptions table without explicit user approval.

> **Rule**: Tightening the quality bar is silent; loosening the bar is a blocking failure.

---

### Step 7: Ratchets (No Decision Required)

When adopting constraints on an existing codebase, do not enforce aspirational numbers that fail immediately:
1. Record today's measured value in `CONSTRAINTS.md` under **Measured Baselines**.
2. Enforce the non-regression invariant: the metric **must not degrade** beyond a $0.5\%$ margin of error.
3. When a PR naturally improves the metric, ratchet the recorded baseline up.

---

## Anti-Rationalization Table

| Rationalization | Reality |
| :--- | :--- |
| *"We can add constraints later after the code settles."* | Code settles around whatever was permitted while it was moving. Establish the floor first. |
| *"The unit tests already verify quality."* | Unit tests prove code satisfies its own logic; they do not measure coverage of new diffs, security vulnerabilities, or dead code. |
| *"We can't hit 80% coverage on this repo."* | Do not set 80%. Measure today's number, lock it into the Ratchet table, and refuse regression. |
| *"Running all checks slows the agent down."* | Placement error. Fast checks run in the edit loop (<5s); heavy suites run at task boundaries. |
| *"I'll just add `#[allow(...)]` or `@ts-ignore` for now to get CI passing."* | Suppressions permanently degrade code health. Fix the root cause or file an exception with an owner and expiration. |

---

## Verification Checklist

- [ ] `CONSTRAINTS.md` exists at repository root with explicit commands and rationales
- [ ] Universal Floor is enforced and passes on current code before new work begins
- [ ] Every declared constraint maps to an executable command that runs today
- [ ] Checks are tiered by cost (<5s fast loop, <90s task completion, CI pre-merge)
- [ ] Ratchets recorded for current baselines without aspirational blockers
- [ ] Git diff inspected against the Floor Guard for silenced linters, skipped tests, or stubs
- [ ] Agent instructions (`GEMINI.md`, `AGENTS.md`, or `CLAUDE.md`) reference `CONSTRAINTS.md`
