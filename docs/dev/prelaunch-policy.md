# Prelaunch development policy

Zoen has not launched and has no production users or production data. Revisit this policy before the first production deployment.

Optimize for the smallest coherent design that represents the product today. Remove obsolete code, schemas, APIs, configuration aliases and transitional paths directly. Do not introduce backward-compatibility shims, legacy aliases, dual reads, dual writes or data-preserving backfills unless the user explicitly requests them.

Internal interfaces are not public compatibility contracts. Update their callers and tests atomically when they change. Development and test data are disposable; recreate affected databases instead of adding product complexity to preserve that data.

Migration history is a replaceable development baseline, but the checked-in migration chain and setup workflow must remain coherent. Do not rewrite an already-applied migration without resetting its affected development and test databases. Consolidating the baseline requires an explicit coordinated change; it is not incidental feature work.

Preserve database invariants, transactional safety, migration idempotence and deterministic setup. These are correctness requirements. Preserve other agents' work and coordinate access to shared test services. Disposable data does not mean a build may reset another test run or erase an installed account without an explicit task to do so.

Keep the rebuild reviewable in its open draft pull request. Update source, callers, generated bindings and meaningful verification together; do not retain an old API merely to keep an obsolete fixture green. The [naming policy](naming.md) and [current checkpoint](zoen-product-progress.md) describe the active source. Dated handoffs and proof receipts retain their original attribution.
