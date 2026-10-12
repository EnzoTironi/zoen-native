# Agent instructions

These instructions replace the previously provided AGENTS.md instructions.

For PRs with UI or UX changes, use `gh --attach` to add before/after screenshots and video. Backend-only PRs do not need visual evidence.

Always update documentation.

The user also explicitly directed development on shared `main`, with direct pushes and one Git coordinator. Follow the [shared-main workflow](docs/dev/shared-main-workflow.md); preserve every other owner’s changes and stage only completed, verified units.

Zoen has not launched and has no production users or data. Follow the [prelaunch policy](docs/dev/prelaunch-policy.md): remove obsolete interfaces directly, update callers and tests atomically, and add no compatibility shims unless explicitly requested. Migration baseline changes require coordinated development/test database resets.
