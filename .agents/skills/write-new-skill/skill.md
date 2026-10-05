---
name: write-a-skill
description: Author structured agent skills following progressive disclosure and workspace standards.
---

# Writing Skills

Author modular, token-efficient agent skills following the Progressive Disclosure Architecture.

## 3-Step Authoring Process

1. **Gather Requirements**:
   - Scope the core capability, triggers, and decision boundaries.
   - Clarify whether deterministic scripts (`scripts/`) or reference docs (`REFERENCE.md`) are needed.

2. **Draft the Skill Structure**:
   ```
   skill-name/
   ├── SKILL.md           # Core instructions (< 50 lines) (required)
   ├── REFERENCE.md       # Deep docs, catalogs & templates (if needed)
   └── scripts/           # Deterministic utility scripts (if needed)
   ```

3. **Verify Quality**:
   - Verify description is under 20–25 words and contains clear trigger keywords.
   - Ensure `SKILL.md` is concise and links to `REFERENCE.md` for extended schemas.
   - Synchronize entry in `skills.json`.

---

## Authoring Guidelines & Templates
* For `SKILL.md` templates, description formatting guidelines, and script conventions, see **[REFERENCE.md](REFERENCE.md)**.
