# GridCraft managed editor — integration in progress

Issue [#445](https://github.com/EnzoTironi/zoen-native/issues/445), under #180.
Pinned engine/UI: `storytold/gridcraft` **b8771d51fb4da60bd48c13cf313d6d6014dc5e66**
(version 0.11.1, MIT OR Apache-2.0). Upstream branding is excluded by its NOTICE;
no brand assets or upstream accounts, storage or collaboration are imported.

The concrete XLSX profile and Core use the existing encrypted FileDoc/chunk,
Item grant, version/CAS and exact receipt paths. `AppBinaryCreateFor` selects a
fixed trusted format; the existing Deck command/wire/resource remain compatible.
No second document database or local accepted-version counter is introduced.
The shared JavaScript host defaults to Deck and explicitly selects Grid only
through its trusted descriptor. Grid is **not registered as available** yet.

`host-bridge/` runs the actual pinned GridCraft `Session`, `cell.set`, calculation,
undo and native XLSX writer. It uses the existing Deck scope/head/proposal/accept
contract. A pending proposal freezes input; only its exact next-version/hash
receipt unfreezes it. Export returns the selected accepted native XLSX bytes,
including after cold open, rather than a newer unsaved draft. No general command,
filesystem, URI or automation API is exposed to agents. The first reviewed
mutation is one bounded `cell.set`; generic upstream commands remain unavailable.
Browser CSP/WASI capabilities and Core authority are still required independently.
The adapter is never proof that an actor has a grant.

`prepare-engine.py` copies this adapter and the **single canonical XLSX profile**
from `crates/miniapps/src/gridcraft{.rs,/}` into a clean owned upstream checkout.
It applies only the embedding command policy hook, checks the exact revision,
refuses dirty/adopted trees, records source hashes, and does not compile, fetch,
install, enable an App or claim a pass. Example preparation:

```sh
python3 miniapps/gridcraft-ui/prepare-engine.py --upstream /absolute/owned/gridcraft
```

After a coordinated compiler lease, generate the lockfile genuinely in that tree,
then qualify with one job, incremental disabled, debug info disabled and the
normal source/resource guard. Initial executable commands are:

```sh
cargo test --locked --offline -p gridcraft-zoen-host -- --nocapture
cargo test --locked --offline -p gridcraft-zoen-host --features ui-tests -- --nocapture
cargo clippy --locked --offline -p gridcraft-zoen-host --all-targets -- -D warnings
cargo run --locked --offline -p gridcraft-zoen-host -- --seed > /absolute/owned/empty.xlsx
```

These commands are a **plan, not results**. The native tests exercise real engine
calculation, manual checkpoint, agent proposal, exact repeat, receipt refusal,
undo, cold reopen and accepted-version XLSX export. The independent standards-
shaped XLSX fixtures in Core tests are adversarial storage fixtures, not engine-
generated seeds. Generate any shipped seed with the actual pinned writer and
record its hash; do not substitute those fixtures for it.

Initial limits: 8 MiB compressed, 16 MiB inflated, 4 MiB per XML part, 128 ZIP
entries, 16 sheets, 40,000 stored cells, 10,000 rows and 256 columns. The headless
executor is deliberately smaller: 512 KiB native document/proposal and 64 KiB
metadata. Macro/external/unknown package parts, malformed references, XML DTDs,
external formulas and unsupported exports refuse explicitly. This is a restricted
managed profile, not a general Excel importer or a claim of upstream parity.

The upstream `?host=parent` browser bridge cannot be used unchanged: it relies on
same-origin parent messages and marks saved before any durable host ACK. A real
managed browser shell, packaged JS/WASM hashes, agent production wiring and actual
iPhone usability/recovery are still pending. No placeholder editor is shipped.
