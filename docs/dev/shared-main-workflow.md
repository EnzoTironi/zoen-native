# DEVELOPMENT WORKFLOW — SHARED MAIN BRANCH

This document records the 12 October 2026 checkpoint. Later work does not follow it. New work uses one branch per unit. The coordinator merges that branch into `main` after the unit's proof, following the pstack playbooks.

## Philosophy

Optimize for fast, coordinated AI-agent development.

Prefer:
- Direct collaboration over branch isolation.
- Shared context over duplicated environments.
- Automated verification over mandatory manual code review.
- Small, continuously integrated changes.
- Parallel execution when tasks are independent.
- Explicit agent coordination over Git-based coordination.

## 1. Git Workflow

- Work on the existing main development branch.
- Do not create feature branches by default.
- Do not create Git worktrees by default.
- Do not open pull requests for routine agent changes.
- Do not require PR-based reviews before continuing development.
- Do not switch branches without explicit authorization.
- Never force-push or rewrite shared history.
- Preserve all existing user and agent changes.

The goal is to eliminate unnecessary branching,
merging, and PR-management overhead.

## 2. Parallel Agent Execution

Use parallel agents whenever tasks can be
completed independently.

Before delegating:

1. Identify independent tasks.
2. Identify dependencies between tasks.
3. Assign each agent a clearly defined responsibility.
4. Define file ownership to avoid overlapping edits.
5. Communicate shared interfaces and requirements.
6. Identify which tasks can execute concurrently.

Agents must coordinate through shared task state
and direct communication.

Do not use separate Git branches as the primary
mechanism for coordinating agents.

## 3. Shared Working Directory

Agents may operate in the same working directory.

Rules:

- Treat the filesystem as shared mutable state.
- Never assume a file has not changed since reading it.
- Check current file contents before modifying them.
- Avoid concurrent modifications to the same file.
- Announce changes to shared interfaces.
- Coordinate before editing files owned by another agent.
- Never overwrite another agent's work.
- Never reset or discard changes to resolve conflicts.

When two tasks require the same files, coordinate
ownership or execute those edits sequentially.

## 4. Git Commit Coordination

Use a single designated coordinator for Git operations.

Implementation agents should not independently
stage, commit, or push shared changes.

The coordinator must:

1. Inspect the current working tree.
2. Identify which changes belong to each task.
3. Ensure no agent is actively modifying files
   included in the commit.
4. Stage only the intended files or hunks.
5. Run appropriate verification.
6. Create a descriptive commit.
7. Push only when authorized.

Never use blanket staging when unrelated changes
may be present.

Never commit incomplete work from another agent.

## 5. Verification Instead of Mandatory PR Review

Do not equate the absence of a PR with the
absence of verification.

Every completed change must be verified using
appropriate automated checks.

Depending on the project, these may include:

- Type checking.
- Linting.
- Unit tests.
- Integration tests.
- End-to-end tests.
- Build verification.
- Security checks.
- Regression tests.

Tests must verify the intended behavior, not merely
confirm that the implementation runs.

Do not weaken tests to make changes pass.

If verification fails, investigate and fix the issue
before declaring completion.

## 6. Adversarial Agent Review

For sufficiently complex or risky changes,
delegate independent verification to another agent.

The reviewing agent should:

- Check implementation against requirements.
- Search for missing edge cases.
- Identify incorrect assumptions.
- Look for regressions.
- Challenge the implementation's correctness.
- Propose additional tests.

The reviewer should not automatically modify
files owned by an active implementation agent.

Communicate findings to the coordinator.

## 7. Continuous Integration of Agent Work

Integrate completed tasks into the shared codebase
as soon as their dependencies and verification allow.

Avoid accumulating large batches of unrelated changes.

Do not wait for all agents to finish before verifying
independent completed work.

Run relevant checks after integration.

Do not disable existing CI without explicit authorization.

If CI exists, preserve it unless a replacement
verification process has been approved.

## 8. Handling Conflicts

When agents encounter overlapping changes:

1. Stop conflicting edits.
2. Identify the current file owner.
3. Communicate the required changes.
4. Assign one agent to integrate them.
5. Re-read the latest file contents.
6. Apply changes without discarding existing work.
7. Re-run affected tests.

Do not resolve conflicts by resetting files,
overwriting another agent's changes, or force-pushing.

## 9. Task Completion

A task is complete only when:

- The requested behavior is implemented.
- Acceptance criteria are satisfied.
- Relevant automated checks pass.
- No known regressions remain.
- Shared interfaces are consistent.
- The coordinator has received the results.

Report:
- What changed.
- Which files changed.
- What was tested.
- Any remaining risks or limitations.

## 10. General Rule

Use agents for implementation, coordination,
testing, and independent verification.

Avoid unnecessary human handoffs.

Maintain a continuously usable shared codebase.

Optimize for verified outcomes rather than
the number of branches, PRs, or reviews performed.
