---
name: generate-spec
description: Generate a feature specification document in docs/specs/ using the GitHub issue description derived from the current branch name. Use when user wants to create, write, or generate a spec for the feature being developed.
---

# Generate Feature Spec

## Workflows

Follow this process to generate a specification document:

1. **Determine the Issue and Branch**:
   - Run `git branch --show-current` to get the current branch name.
   - Extract the issue number from the beginning of the branch name (e.g., branch `50-resend-login-verification-code` -> issue `50`).
   - The target spec file will be `docs/specs/spec-<branch_name>.md`.

2. **Fetch Issue Information**:
   - Run `gh issue view <issue_number> --json title,body` to get the issue details. Note that the output might contain progress characters before the JSON; ignore them and extract the JSON content.
   - **CRITICAL**: The information in the GitHub issue is NOT to be used verbatim in the spec. Use the GitHub issue description strictly to *inform* what the spec should be.

3. **Research the Codebase**:
   - Before writing the spec, you MUST explore the codebase to understand how this feature should be implemented technically.
   - Look for existing specs in `docs/specs/` to understand the expected format, detail level, and structure.
   - Use tools like `grep_search` and `view_file` to find the relevant modules, files, and components that will need to be changed.
   - **CRITICAL**: Do not stop at the first relevant file. Think about the full stack. Identify ALL areas that will be impacted by the feature (e.g., database models, backend APIs, frontend UI components, admin interfaces) and ensure they are all documented in the technical implementation.

4. **Align on Design (Grill Me)**:
   - Before generating the spec, use the `grill-me` skill to ask the user clarifying questions about the design, implementation, edge cases, and requirements.
   - Interview the user relentlessly about every aspect of the plan until you reach a shared understanding. Walk down each branch of the decision tree, resolving dependencies between decisions one-by-one.
   - Ensure all edge cases and full-stack implications are resolved before writing.

5. **Generate the Spec**:
   - Create the spec file `docs/specs/spec-<branch_name>.md`.
   - The spec must be highly detailed and typically include the following structure:
     - `# Spec <Issue Number>: <Title>`
     - `## Overview`: A high-level description of the feature based on the issue.
     - `## Requirements`: Detailed flow, exclusions, and included fields.
     - `## Edge Cases`: Explicitly list out any edge cases, failure states, or unexpected user interactions that must be handled.
     - `## Technical Implementation`: Extremely detailed breakdown. Include the specific modules, files, components, and functions to be modified or created. Outline the state management, API endpoints, database queries, and any other technical details needed to achieve the feature.
     - `## Unit Test Cases`: Define the specific unit tests that should be generated to verify the feature works correctly, including testing the edge cases.
     - `## Acceptance Criteria`: A comprehensive checklist of conditions that must be met for the feature to be considered complete.
   - Write the generated content to the target spec file.

6. **Review**:
   - Once written, let the user know the spec has been generated and is ready for their review.
