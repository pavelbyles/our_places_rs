---
name: pr-analyzer
description: Creates a pull request in GitHub, runs the sanity-check workflow, and performs a comprehensive 8-point analysis of the code changes. Use when the user asks to create a PR, review code changes, or perform PR analysis.
---

# PR Analyzer & Creator

## Process

When invoked to analyze changes and create a PR, follow these steps systematically:

1. **Run the Sanity Check Workflow**
   - View and execute the instructions in the workflow file: `workflows/sanity-check-workflow.md`.
   - Ensure the code passes all sanity checks before proceeding.

2. **Run Security Audit**
   - Execute the `cargo audit --ignore RUSTSEC-2024-0436 --ignore RUSTSEC-2023-0071` command in the terminal to check the Rust dependencies for any known CVEs (security vulnerabilities).
   - Record the results of this audit for your final analysis.

3. **Perform the 8-Point Analysis**
   Compare the code changes against the user's stated intent for the change. You must analyze the change across the following 8 dimensions:
   1. **Core Logic**: Does the code accurately and completely implement the intended core logic?
   2. **Edge Cases**: Have edge cases and boundary conditions been properly handled?
   3. **Business Logic & Side Effects**: Does the change align with business rules? Are there any unintended side effects elsewhere in the application?
   4. **Readability**: Is the code clear, well-structured, and easy for other developers to read?
   5. **DRY Principle**: Are there any violations of the DRY (Don't Repeat Yourself) principle? Is there duplicated code that should be abstracted?
   6. **Complexity**: Is the implementation unnecessarily complex? Can it be simplified?
   7. **Documentation**: Does sufficient documentation exist for the modified or newly introduced code (e.g., docstrings, comments)?
   8. **Security Vulnerabilities**: Summarize the findings from the `cargo audit` run and note any other potential security flaws in the code logic.

4. **Create the Pull Request**
   - Ensure the user's changes are committed and pushed to a branch.
   - First, write your complete 8-point analysis to a temporary markdown file in your `scratch/` directory (e.g., `scratch/pr_body.md`).
   - Create the PR using the GitHub CLI: `gh pr create --title "<Title>" --body-file <path_to_scratch_file>`.
   - **Crucial Fallback**: If the `gh` CLI creates the PR but fails to attach the body (or throws a GraphQL deprecation error), immediately use the GitHub API to update it:
     `gh api -X PATCH repos/{owner}/{repo}/pulls/{pr_number} -F body=@<path_to_scratch_file>`
   - Always verify the PR description was successfully attached.

5. **Output the Final Summary**
   - Present a detailed summary to the user documenting your findings for **each of the 8 analysis items**. 
   - You must explicitly list all 8 items and provide your findings for each one so the user has a clear record of the analysis.
