# Prelaunch development policy

Zoen has not launched and has no production users or production data. Revisit this policy before the first production deployment.

Optimize for the smallest coherent design that represents the product today. Remove obsolete code, schemas, APIs, configuration aliases and transitional paths directly. Do not introduce backward-compatibility shims, legacy aliases, dual reads, dual writes or data-preserving backfills unless the user explicitly requests them.

Internal interfaces are not public compatibility contracts. Update their callers and tests atomically when they change. Development and test data are disposable; recreate affected databases instead of adding product complexity to preserve that data.

Migration history is a replaceable development baseline, but the checked-in migration chain and setup workflow must remain coherent. Do not rewrite an already-applied migration without resetting its affected development and test databases. Consolidating the baseline requires an explicit coordinated change; it is not incidental feature work.

Preserve database invariants, transactional safety, migration idempotence and deterministic setup. These are correctness requirements. Preserve other agents' committed work. New work uses one branch per unit and the pstack playbooks. The coordinator merges that branch into `main` after the unit's proof. The [shared-main workflow](shared-main-workflow.md) records only the 12 October checkpoint.

The 12 October handoff does not consolidate migrations or complete the naming/API cutover. It records the current source and the remaining atomic caller migrations in [the handoff](handoff-2026-10-12.md).
