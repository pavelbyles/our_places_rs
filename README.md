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


# Our Places (`our_places_rs`)

High-performance, full-stack short-term property rental platform for luxury villas and apartments, structured as an Isomorphic Rust Monorepo targeting GCP Cloud Run scale-to-zero workloads.

---

## 🔄 AI-Native SDLC & Agent Skills Reference

The engineering workflow operates across 6 artifact-driven stages in an AI-Native Software Development Lifecycle (SDLC):

| SDLC Stage | Primary Artifact | Associated Skills | Execution Role & Trigger |
| :--- | :--- | :--- | :--- |
| **Stage 1: Plan** | `intent.md` | [`draft-intent`](.agents/skills/draft-intent/SKILL.md)<br>[`router`](.agents/skills/router/SKILL.md)<br>[`grill-me`](.agents/skills/grill-me/skill.md)<br>[`create-worktree`](.agents/skills/create-worktree/SKILL.md) | **Ideation & Scoping**: Brainstorm raw ideas, scope MVP boundaries, capture non-negotiable invariants, and isolate git worktrees. |
| **Stage 2: Design** | `spec.md` | [`generate-spec`](.agents/skills/generate-spec/SKILL.md)<br>[`grill-me`](.agents/skills/grill-me/skill.md)<br>[`assumption-review`](.agents/skills/assumption-review/SKILL.md)<br>[`edge-case-analysis`](.agents/skills/edge-case-analysis/SKILL.md)<br>[`failure-scenario-analysis`](.agents/skills/failure-scenario-analysis/SKILL.md)<br>[`resilience-exploration`](.agents/skills/resilience-exploration/SKILL.md)<br>[`security-review`](.agents/skills/security-review/SKILL.md)<br>[`security-posture-assessment`](.agents/skills/security-posture-assessment/SKILL.md)<br>[`risk-assessment`](.agents/skills/risk-assessment/SKILL.md)<br>[`vulnerability-analysis`](.agents/skills/vulnerability-analysis/SKILL.md) | **Specification & Policy Review**: Compress requirements and technical design into a single session; stress-test edge cases, failure blast radiuses, OWASP threats, and lock data models. |
| **Stage 3: Build** | `plan.md`<br>Source Code / Diff | [`rust-core`](.agents/skills/rust-core/SKILL.md)<br>[`monad-design`](.agents/skills/monad-design/SKILL.md)<br>[`topcoat`](.agents/skills/topcoat/SKILL.md)<br>[`daisyui`](.agents/skills/daisyui/SKILL.md)<br>[`lint-hunter`](.agents/skills/lint-hunter/SKILL.md)<br>[`general-debug`](.agents/skills/general-debug/SKILL.md)<br>[`write-new-skill`](.agents/skills/write-new-skill/skill.md)<br>[`handoff`](.agents/skills/handoff/skill.md) | **Implementation**: Execute code implementation starting from `plan.md`; enforce panic-free monadic Rust (`Option`/`Result`), tri-currency math, and SSR/UI components. |
| **Stage 4: Test** | Test Runs<br>`evals_results.json` | [`general-debug`](.agents/skills/general-debug/SKILL.md)<br>[`lint-hunter`](.agents/skills/lint-hunter/SKILL.md)<br>`chrome-devtools`<br>`a11y-debugging` | **Verification & Evals**: Run local test suites and compile checks; execute synthetic benchmark eval suites against agent skills to prevent prompt regressions; audit browser accessibility. |
| **Stage 5: Deploy** | `REVIEW.md`<br>Pull Request | [`pr-analyzer`](.agents/skills/pr-analyzer/SKILL.md)<br>[`pr-remediation`](.agents/skills/pr-remediation/SKILL.md) | **PR Review & Remediation**: Run 8-point automated code quality review, autonomously sweep and fix review comments/broken CI checks, and sync documentation post-ship. |
| **Stage 6: Maintain** | Incident `intent.md` | [`auto-triage-incident`](.agents/skills/auto-triage-incident/SKILL.md) | **Closed-Loop Incident Triage**: Ingest telemetry/log anomalies and metric breaches, isolate root causes, and write an `intent.md` proto-spec to restart Stage 1. |

---

## 🏛️ Architecture Documentation

* **System Architecture**: See [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) for monorepo crate taxonomies, database locking, and tri-currency pricing flows.
* **Agent Rules & Invariants**: See [`.agents/AGENTS.md`](.agents/AGENTS.md) for hard monorepo rules, performance budgets (<300ms), and token efficiency directives.
