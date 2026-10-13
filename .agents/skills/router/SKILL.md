---
name: router
description: Classify user intent and route complex tasks to dedicated specialist skills or workflows.
---

# Agent Router

Parse user intent and route tasks to the single most appropriate specialist skill or workflow across the AI-native SDLC.

## Strict Dispatch Constraints

* **Single-Skill Selection**: Select **exactly one** primary specialist skill per user request.
* **No Cascade Loading**: Never read multiple overlapping skill files into context simultaneously unless executing a formal multi-stage workflow.
* **Progressive Disclosure**: Only read the primary skill's `SKILL.md` first; load companion `REFERENCE.md` files only on demand.

---

## Stage-Aware Routing Matrix

### Stage 1: Plan
* "draft intent", "proto-spec", "new feature idea", "scope requirements" $\rightarrow$ `draft-intent`
* "create worktree", "new branch for issue", "isolate worktree" $\rightarrow$ `create-worktree`
* "constraints", "quality bar", "quality gates", "CONSTRAINTS.md", "enforce standards" $\rightarrow$ `constraint-driven-development`
* "task breakdown", "planning tasks", "break down spec", "implementation plan", "tasks/plan.md" $\rightarrow$ `planning-and-task-breakdown`

### Stage 2: Design
* "write spec", "generate spec", "feature specification" $\rightarrow$ `generate-spec`
* "grill me", "interview on design", "stress-test plan", "decision tree" $\rightarrow$ `grill-me`
* "security audit", "OWASP review", "threat model", "vulnerabilities", "hardening" $\rightarrow$ `security-and-hardening`
* "edge case", "boundary condition", "extreme input", "type limit" $\rightarrow$ `edge-case-analysis`
* "failure scenario", "outage blast radius", "dependency failure" $\rightarrow$ `failure-scenario-analysis`
* "challenge assumptions", "unstated assumptions", "what if wrong" $\rightarrow$ `assumption-review`
* "system resilience", "fault tolerance", "recovery under disruption" $\rightarrow$ `resilience-exploration`
* "risk assessment", "risk matrix", "likelihood and impact" $\rightarrow$ `risk-assessment`
* "security posture", "compliance readiness", "maturity rating" $\rightarrow$ `security-posture-assessment`
* "vulnerability triage", "CVSS scoring", "scanner findings" $\rightarrow$ `security-and-hardening` (or `vulnerability-analysis`)

### Stage 3: Build
* "build", "build auto", "autonomous build", "run plan", "execute tasks" $\rightarrow$ `/build` (workflow)
* "implement Rust", "pricing logic", "database entity", "sqlx", "actix handler" $\rightarrow$ `rust-core`
* "monad", "railway-oriented", "Result pipeline", "combinator chain" $\rightarrow$ `monad-design`
* "topcoat", "SSR template", "view! macro", "path_param!", "HTMX swap" $\rightarrow$ `topcoat`
* "daisyui", "tailwind styling", "modal", "card", "navbar", "drawer" $\rightarrow$ `daisyui`
* "verify docs", "official documentation", "source driven", "framework patterns", "docs.rs" $\rightarrow$ `source-driven-development`
* "vertical slice", "incremental implementation", "step-by-step build", "break down implementation", "slice feature" $\rightarrow$ `incremental-implementation`
* "simplify code", "reduce complexity", "clean up code", "chesterton's fence", "refactor for clarity" $\rightarrow$ `code-simplification`
* "instrument code", "structured logging", "RED metrics", "tracing span", "telemetry", "alert runbook" $\rightarrow$ `observability-and-instrumentation`
* "author skill", "write skill", "create new skill" $\rightarrow$ `write-new-skill`
* "handoff session", "compact context", "summarize state for next agent" $\rightarrow$ `handoff`

### Stage 4: Test
* "TDD", "test-driven", "red-green-refactor", "write tests first", "prove-it" $\rightarrow$ `test-driven-development`
* "compiler error", "borrow checker", "lifetime issue", "E0..." $\rightarrow$ `lint-hunter`
* "runtime panic", "logic bug", "test failing", "debug helper" $\rightarrow$ `general-debug`
* "playwright", "e2e tests", "browser test", "portal test" $\rightarrow$ `playwright-e2e`
* "booking audit", "financial audit", "row lock audit" $\rightarrow$ `/audit-booking-flow`
* "cold start audit", "Cloud Run performance", "latency budget" $\rightarrow$ `/cloudrun-perf-audit`
* "sonarqube", "sonarcloud", "deepsource", "static analysis", "doc coverage", "hygiene" $\rightarrow$ `sonarqube-guard`
* "eval skills", "skill benchmark regression", "eval suite" $\rightarrow$ `/eval-skills`

### Stage 5: Deploy
* "create PR", "open pull request", "analyze PR changes", "ship PR" $\rightarrow$ `pr-analyzer` (or `/ship-pr`)
* "fix PR comments", "remediate PR", "failing CI check on PR" $\rightarrow$ `pr-remediation` (or `/remediate-pr`)
* "document release", "sync docs post-ship", "release notes" $\rightarrow$ `/document-release`

### Stage 6: Maintain
* "triage alert", "production incident", "log anomaly", "metric breach" $\rightarrow$ `auto-triage-incident`
* "investigate root cause", "systematic debugging on live issue" $\rightarrow$ `/investigate`
* "retro", "retrospective", "session retro", "agent environment improvement", "workflow retro" $\rightarrow$ `retro`

---

## Anti-Patterns & Negative Triggers

* **DO NOT** activate `pr-analyzer` or `security-and-hardening` for simple syntax checks or code explanations.
* **DO NOT** activate `draft-intent` if a feature branch and formal `spec.md` already exist (proceed to `generate-spec` or `rust-core`).
* **DO NOT** activate `auto-triage-incident` for local compiler warnings (use `lint-hunter`).

---

## Output Format
```
> ROUTING: [Skill or Workflow Name]
> SDLC STAGE: [Stage 1-6]
> REASONING: [Brief 1-sentence explanation]
```