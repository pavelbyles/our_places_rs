# Skill Authoring Reference

## 1. Universal `SKILL.md` Template

```md
---
name: skill-name
description: [Action verb] [what it does] using [tools/methods] for [use case]. (Max 20-25 words)
---

# Skill Title

[One sentence summary of capability].

## Core Workflow / Rules

1. **Step 1**: [Actionable instruction]
2. **Step 2**: [Decision criteria]

## Decision Matrix / Checklist

- [ ] [Verification item 1]
- [ ] [Verification item 2]

---

## Detailed References
* For detailed schemas, examples, and edge cases, see **[REFERENCE.md](REFERENCE.md)**.
```

---

## 2. Description Formatting Standards

The `description` in YAML frontmatter is injected into the system prompt across all sessions.

* **Target Length**: 12–25 words.
* **Tone**: Declarative, third-person.
* **Essential Elements**: Primary action, domain area, and explicit trigger keywords.

### Comparison Examples:
* **Good**: `Diagnose production errors, metric breaches, and log anomalies, generating an actionable intent.md proto-spec to restart the SDLC loop.`
* **Bad**: `This skill helps with diagnosing issues when something goes wrong and can be used when users ask for help fixing bugs or looking at logs.`

---

## 3. When to Add Deterministic Scripts (`scripts/`)

Add Python/Bash scripts under `scripts/` when:
* Operations are fully deterministic (string slugification, git worktree creation, validation).
* Generating code repeatedly burns excessive tokens and introduces variance.
* Strict exit code handling is required for automated pipelines.
