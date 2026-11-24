---
name: eval-skills
description: Multi-agent synthetic skill & rule benchmark evaluation — orchestrates a fleet of parallel evaluator subagents to regression-test skills, system rules, and AGENTS.md invariants across isolated context windows.
---

# /eval-skills — Multi-Agent Skill & Rule Benchmark Evaluation Fleet

## Goal
Regression-test agent skills, `AGENTS.md`, and system rules against a standardized suite of synthetic benchmark prompts using a **Multi-Agent Evaluation Fleet (Map-Reduce)**. 

By executing benchmark prompts in isolated subagent context windows, the evaluation prevents cross-prompt context bleeding, verifies behavioral compliance with 100% fidelity, and blocks regressions before committing workflow changes.

## When to Use
Run `/eval-skills` whenever:
- Modifying or adding files in [`.agents/skills/`](../) (e.g. `rust-core`, `monad-design`, `grill-me`, `auto-triage-incident`, `sonarqube-guard`, `retro`).
- Updating project guidelines in `AGENTS.md` or `.agents/rules/`.
- Preparing a PR that touches AI workflow definitions or prompt instructions.

---

## Multi-Agent Fleet Architecture

```mermaid
flowchart TD
    Trigger[Skill / Rule Modification] --> Coordinator[Eval Fleet Coordinator]
    
    subgraph Parallel Evaluator Fleet (Map Phase)
        Coordinator --> Worker1[Evaluator 1: rust-core Suite]
        Coordinator --> Worker2[Evaluator 2: monad-design Suite]
        Coordinator --> Worker3[Evaluator 3: grill-me Suite]
        Coordinator --> Worker4[Evaluator 4: auto-triage-incident Suite]
        Coordinator --> Worker5[Evaluator 5: sonarqube-guard Suite]
        Coordinator --> Worker6[Evaluator 6: workflow & intent Suites]
    end
    
    Worker1 --> Collect[Collect Benchmark Outputs & Assertions]
    Worker2 --> Collect
    Worker3 --> Collect
    Worker4 --> Collect
    Worker5 --> Collect
    Worker6 --> Collect
    
    Collect --> Reducer[Reducer: Run Assertion Matrix via eval_runner.py]
    Reducer --> PassGate{100% Critical Invariants Passed?}
    PassGate -->|Yes| Green[Approved: Commit Skill Changes]
    PassGate -->|No| Block[Block Commit: Dispatch Remediation to Author]
```

### Roles & Responsibilities

| Role | Execution Model | Responsibilities |
| :--- | :--- | :--- |
| **Eval Fleet Coordinator** | Supervisor / Reducer | Identifies modified skills, dispatches worker subagents, collects completion texts, executes the assertion matrix, and outputs the final compliance scorecard. |
| **Evaluator Subagents (1–N)** | Isolated Worker Agents | Each worker receives a clean context window, loads only the target skill's `SKILL.md`, and runs the synthetic test prompt to produce a candidate completion. |
| **Assertion Validator** | Automated Runner | Validates completions against regex and keyword rules defined in [`.agents/evals/skills_benchmarks.json`](../../evals/skills_benchmarks.json) via [`.agents/evals/eval_runner.py`](../../evals/eval_runner.py). |

---

## Multi-Agent Evaluation Protocol

### Phase 1: Target Discovery (Coordinator)
1. Read the list of registered benchmarks:
   ```bash
   python3 .agents/evals/eval_runner.py --list
   ```
2. Determine test targets:
   - **Targeted Run**: Evaluate only modified skills (e.g. `/eval-skills rust-core`).
   - **Full Regression**: Evaluate all 9 suites across the workspace.

---

### Phase 2: Parallel Subagent Execution (Map Phase)

The Coordinator dispatches worker subagents in parallel. Each subagent runs in an isolated context window with the dispatch template:

```
You are an Evaluator Subagent testing the behavioral compliance of the skill: [{skill_id}].
Load the skill instructions from .agents/skills/{skill_id}/SKILL.md.
Do NOT load instructions or context from other skills.

Execute this synthetic benchmark prompt:
"{prompt}"

Respond ONLY with the completion text as if you were an agent assisting a user.
```

#### Benchmark Suites & Prompts

1. **`rust-core` Evaluator Subagent**
   - **Prompt 1.1 (Zero-Unwrap & Type Safety)**:
     > *"Write an Actix-web handler in Rust that parses a JSON payload containing `user_id` (UUID) and `amount` (string), converts `amount` to Decimal, and returns a JSON response."*
     - Assertions: No `.unwrap()` or `.expect()`; uses `rust_decimal::Decimal`; returns `Result<HttpResponse, AppError>`.
   - **Prompt 1.2 (Async Non-Blocking Offload)**:
     > *"Write a Rust function that performs bcrypt password hashing inside an Actix-web async request handler."*
     - Assertions: Uses `tokio::task::spawn_blocking`; avoids blocking Tokio worker threads.

2. **`monad-design` Evaluator Subagent**
   - **Prompt 2.1 (Railway-Oriented Combinator Chains)**:
     > *"Implement a domain service function `process_booking_payment` that takes a `BookingRequest`, validates the date range, checks property availability, applies a discount code if present, and commits the hold. Do NOT use deeply nested if-let or match blocks."*
     - Assertions: Uses `.and_then()`, `.map()`, `.transpose()`; rejects nested `match`/`if let`; returns pure `Result<T, E>`.

3. **`grill-me` Evaluator Subagent**
   - **Prompt 3.1 (Decision-Tree Interrogation)**:
     > *"I want to add multi-currency payouts to hosts in Jamaica. Let's start building it immediately."*
     - Assertions: Refuses to write code immediately; presents branching decision-tree questions with recommended options.

4. **`auto-triage-incident` Evaluator Subagent**
   - **Prompt 4.1 (Anomaly to Intent Spec)**:
     > *"Triage this production incident: `ERROR booking_api: deadlock detected on property_availability row locks during concurrent hold initiation for villa_id=987`."*
     - Assertions: Classifies as P0 - Critical; generates `docs/intent/intent-*.md`; includes invariants check and reproduction plan.

5. **`sonarqube-guard` Evaluator Subagent**
   - **Prompt 5.1 (Static Analysis & Doc Coverage)**:
     > *"Define a public Rust enum `NotificationChannel` with variants `Email`, `Sms`, and `Push`, and a public struct `NotificationConfig` with fields `channel` and `api_key_env`. Ensure the code adheres to SonarQube and DeepSource quality standards."*
     - Assertions: `///` comments on enum and every variant; `///` comments on struct and every field; zero unwrap.

6. **Workflow & Protocol Evaluator Subagents**
   - `draft-intent`: Evaluates originator idea conversion to `intent.md`.
   - `pr-remediation`: Evaluates unwrap fix and sanity-check execution before pushing.
   - `retro`: Evaluates session friction capture and concrete `AGENTS.md` / hook improvements.

---

### Phase 3: Assertion Reduction & Scoring (Reducer)

The Coordinator gathers candidate completions and executes the assertion matrix:

```bash
python3 .agents/evals/eval_runner.py --verify
```

#### Scoring Gates

| Invariant Class | Pass Threshold | Action on Breach |
| :--- | :---: | :--- |
| **Critical Domain Invariants**<br>• Zero floats in money (`rust_decimal`)<br>• Zero `.unwrap()` / `.expect()` in production<br>• Concurrency row-level locks (`SELECT FOR UPDATE`)<br>• Mandatory grill-me interview before coding | **100% Pass**<br>*(Zero tolerance)* | **BLOCK COMMIT IMMEDIATELY**.<br>Dispatch remediation task to skill author subagent. |
| **Stylistic Guidelines**<br>• Monadic combinator chaining depth<br>• Terse markdown summary formatting | **$\ge 90\%$ Pass** | Non-blocking warning; log improvement suggestion. |

---

### Phase 4: Output Scorecard

The Fleet Coordinator outputs the multi-agent evaluation scorecard:

```markdown
### 📊 Multi-Agent Skill Benchmark Scorecard

| Suite ID | Benchmark Name | Evaluator Worker | Assertions | Status | Critical Invariant Breach |
| :--- | :--- | :--- | :---: | :---: | :--- |
| `rust-core` | Zero-Unwrap & Error Propagation | Evaluator-1 | 4/4 | PASS | None |
| `rust-core` | Async Non-Blocking I/O | Evaluator-1 | 2/2 | PASS | None |
| `monad-design` | Railway Combinator Pipeline | Evaluator-2 | 3/3 | PASS | None |
| `grill-me` | Decision-Tree Interrogation | Evaluator-3 | 2/2 | PASS | None |
| `auto-triage` | Telemetry to Intent Artifact | Evaluator-4 | 3/3 | PASS | None |
| `sonarqube-guard` | Static Analysis & Doc Coverage | Evaluator-5 | 2/2 | PASS | None |
| `draft-intent` | Idea to Proto-Spec | Evaluator-6 | 3/3 | PASS | None |
| `pr-remediation` | Review Comment Remediation | Evaluator-6 | 3/3 | PASS | None |
| `retro` | Session Retrospective Fixes | Evaluator-6 | 3/3 | PASS | None |
| `plan-eng-review` | Multi-Agent Plan Review Panel | Evaluator-7 | 3/3 | PASS | None |

**Overall Score**: 28/28 Assertions Passed (100%)
**Gate Result**: APPROVED — Ready to commit skill updates.
```
