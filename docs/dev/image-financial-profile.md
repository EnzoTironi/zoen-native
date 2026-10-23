# Image financial profile foundation

Issue #317 extends the provider seam in #307 toward #179. This is inert pricing:
no task admission, SQL reservation, settlement transaction, runtime constructor or
paid dispatch is enabled. The existing finance authority, database and journals
remain the only intended authority. A complete authenticated image task/binding
and financial continuity are still required before enabling the product.

The host supplies `ImagePriceProfile`: an explicit version, USD micro-unit scale,
endpoint, model and credential reference; exact one-PNG specification and byte
bounds; four input/output text/image tariffs per million tokens; and a positive
configured reservation in existing `i64` money units. There is no default model,
price lookup, invented free price or output-token ceiling. The endpoint exposes
no `max_output_tokens`; reservation is a policy hold, not a provider guarantee.

`quote` verifies the gateway image profile, the exact output specification and
compatible bounded dispatch. Its digest binds the full profile digest, context,
request digest and actual descriptor. It returns an immutable `ImagePriceQuote`,
without Deserialize or DispatchAuthority. The quote can describe a reservation;
it does not create a durable hold or permit an HTTP call.

`charge` returns only a supported incurred configured obligation or Unknown.
It requires matching context/request digest, complete bounded evidence, successful
HTTP, and consistent normalized/raw counters. An optional reported model must
match; when Images omits a model echo, the dispatched request binding supplies
the configured tariff identity without claiming that it was provider-reported.
Substituted response specification or unsupported nonzero cache/reasoning usage
remains Unknown. Differential cached-input pricing is not implemented.

All arithmetic is checked integer arithmetic shared with existing chat pricing:
sum `tokens * configured_rate` across the supported tariffs, then ceil once to
micro-units by dividing by one million. When text/image tariffs differ, both
reported detail counters and their matching total are necessary. Equal tariffs
can use reported totals alone, without inventing missing detail counters. Missing,
invalid or incomplete usage remains Unknown; reported zero can produce zero.
Malformed output can still incur a valid charge. A supported charge exceeding
the configured reservation is returned in full, never capped. Overflow fails
closed; callers must retain the uncertain obligation rather than release it.

The existing `Binding`, `Capture`, `VerifiedStep` and `Session` moved into the
private `admission` module to keep the touched root below20KB. Their field order,
Serde representation, fencing, reserve/claim/dispatch flow and private scope are
preserved. Native custody, RuntimeAuthority opening and SQL are unchanged.

Hermetic pricing tests cover specification and scope changes, incompatible
profiles, integer rounding/overflow, missing/contradictory usage, reported zero,
and charge above reserve. These do not prove a SQL hold, owner budget enforcement,
provider delivery, cost reconciliation or creation of an encrypted Image Item.
