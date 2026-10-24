# Canonical plain DOCX foundation

Tracks #502 / #180; neither is complete. Executed source:
`d12bcea7434c81d8cc879c453cd80bd6de54f46c`. Final evidence-only commit does not
change its producer, Core, Miniapps, worker or CLI consumer inputs.

The normal binary Format/Item/FileDoc/CAS/outbox/receipt code now recognizes
WordCraft. The format is the same bounded plain DOCX profile as the worker;
metadata contains format/revision and byte facts, never document text. No schema,
second journal, provider, browser resource or App installation was introduced.
Catalogue `available()` remains false. Existing deployment configuration can load
a signed `wordcraft-canonical-v1` worker at the fixed upstream revision; it does
not advertise a Word agent tool or complete the owner-approval journey.

## Real regression and correction

The previous qualified native worker accepted U+FFFE/U+FFFF in replacement text
and produced a DOCX whose reopened text had lost those characters. Evidence:
`wordcraft/canonical-character-red.json`, SHA-256
`7f08c5863080a4632e041cdc39bc727b685597d11b9f9f33e052b50e608f3958`.

Validation now follows all XML 1.0 scalar ranges implemented by the pinned writer,
plus the existing LF/tab/control profile. Supplementary-plane noncharacters that
are valid XML remain supported. Every proposal also requires encode -> actual
reader -> exact requested-text equality, before retaining any pending proposal.
The distinct `encoded_text_mismatch` refusal guards future serializer losses.
Seven actual engine tests cover the boundaries, Unicode/ZWJ, entities, LF/tab,
cold reopens, immutable accepted bytes and original ACK/refusal behavior. The real
canonical consumer requires **exact `unsupported_text`** for the two observed
lossy inputs; an unrelated timeout/error cannot satisfy that assertion.

## Executed qualification

External receipt root:
`/Users/enzotironi/.codex/builds/zoen-artcraft-next-v500/wordcraft/`.
All Cargo invocations used the normal source guard, a private target, jobs=1,
debug=0 and incremental=0. No shared v410, services, frozen SDK or device.

| Scope | Actual result |
| --- | --- |
| Four packages, all-target Clippy | Miniapps, Core, agentd and CLI PASS at `fd71fbaa`; 51.614s, `canonical-check-03` |
| Miniapps closed DOCX profile | 3 PASS, zero ignored, `canonical-check-02` |
| Core whole binary-document suite | 17 PASS, zero ignored, `canonical-check-03`; actual encrypted file chunks, MIME/format isolation, rollback, revocation, CAS, later-version/original receipt and selected historical bytes |
| agentd worker dispatch | 2 PASS, zero ignored, `canonical-check-03` |
| CLI loader/configuration filter | 2 PASS and **4 existing ignored Deck component cases**, `canonical-check-03`; no new ignore or changed skip policy |
| Core client-types/all-target Clippy | PASS, 55.177s, `canonical-check-04` |
| Final real consumer Clippy/build | PASS, 28.325s / 91.029s, `canonical-check-06` |
| New upstream producer | Native all-target Clippy PASS 14.194s; 7 tests PASS/0 ignored, 0.08s execution/152.830s guard; WASIp2 bin Clippy PASS 13.667s; release components PASS 290.815s, `wasi-check-03` |
| Actual signed WASIp2 -> canonical Core | PASS, 182.694s total, `canonical-runtime-01`; described below |

Core tree `4b631bf5dfb1f00cb6d84216f4a4f5d59f8dfc4e`, Miniapps tree
`eef1b943202de9c547235c2ee12be99522297667`, and agentd tree
`2de511a5c28af4a32bfc6253e3e77905f1b242d8` are byte-identical between their
respective qualification sources and final executed `d12bcea7`. Later changes
strengthened only the CLI example; its final Clippy/build ran separately. No
full-workspace, full-upstream CI or App check is claimed.

Result SHA-256 values:

- `canonical-check-03/result.json`: `16fcaa8185a6b2d2e739a017f20764020db42132f9c5c062c35cba38c6570f36`
- `canonical-check-04/result.json`: `7128596501a24b4e3330e46fff6c272134801a214fe854420670af0fa617478b` (Core types PASS followed by example lint RED)
- `canonical-check-06/result.json`: `6943e33019c8c1f40c99e7db0c6cc4fcfbb6ff78b41b02a1f6ad54693aed6f3a`
- `wasi-check-03/result.json`: `926156b2f436cf1c8816f71aca577a71415fda56bf30f7e08692861a9b956293`
- `canonical-runtime-01/result.json`: `5eb20e6e57bc28feb7b186fffb657ec8cc4f2da7ca877406699dbe92501608f5`
- `canonical-runtime-01/actual/result.json`: `2b583ae22155bb2eab1970a00cecf6d6cb60b91ffee18996fc7e6dfdd4ad7386`

## Actual canonical consumer

`crates/zoen-cli/examples/wordcraft_canonical_check.rs` uses the production signed
`DeckcraftWorker::load_for("wordcraft")`, the existing WasmTier and ordinary Core
account/group/binary APIs. Its account and publisher are explicitly synthetic;
Core assigns the actual Item, actor, version and hash. No File/Item identifiers
or DTOs are fabricated. Absolute component, actual writer seed and new output
paths are its only inputs:

```sh
cargo clippy -p zoen-cli --example wordcraft_canonical_check -- -D warnings
cargo build -p zoen-cli --example wordcraft_canonical_check
ABS_CHECKER ABS_NEW_WORKER_COMPONENT ABS_ACTUAL_WRITER_SEED ABS_NEW_OUTPUT
```

The executed consumer checks the engine/revision and format boundary, creates
once through normal Core, runs the actual engine command and its identical
retry, requires the two exact profile refusals, saves the actual proposal as v2,
then advances the same Item to v3. It drops and reopens the actual local Core
client/SQLite with its retained synthetic vault, observes the original v2 receipt
(current v3), reads exact v1 bytes, refuses a foreign selected hash and a stale
new mutation, and reopens accepted v2 bytes in a fresh actual WASI store. That
last read compares the **exact observed scope/head and original Unicode text**.
This is cold Core/client/store recovery, not a physical-device/process-restart,
remote-relay or certified-agent approval proof.

Actual accepted DOCX SHA-256:
`9b9c37ce8a2c937eb0fe94feceabfd8392e29ad845fd5662c697bf4d378f77c4`.

New worker: **30,959,549 bytes**, component header `0061736d0d000100`, SHA-256
`a9200940b3ec522e645e5d342304267d041f43296129d44c3a0922f6f5bfbb2a`.
Prepared source: `fbb79c45408d8987bd9cbaf0123289596e796058`. Its genuinely
resolved lock adds only the host package, with no external pin/version/checksum
change. Normal selected graph has no JS/GUI/GPU dependency.

The rebuilt clock probe is **byte-identical** to the one whose three actual clocks
already passed under #510 (`3737f8d10921f580834260a5fda5c5c66368ff65d205d2b43bdcfb02cc8ba14e`).
It was not executed again and no extra clock pass is claimed. Old sealed
components and all original receipts remain unchanged.

The real cold worker JIT took **181.489s**, separate from execution. Each call
retains **5s / 2 billion fuel / 256 MiB**. Verified compiled workers must be
retained/prewarmed in the existing tier before deployment readiness; no second
runtime, unsafe AOT deserialization or latency guarantee is added here.

## Preserved failures and remaining product work

Original failures are retained: initial external-wrapper NameError before Cargo;
Core/Miniapps Clippy doc-backtick diagnostic; one Core fixture that incorrectly
expected v1 and its intentionally restored v3 to have different hashes; and
example `cmp_owned` diagnostics (one repeated attempt before the source edit
actually landed). The fixture now uses the genuinely different v2 hash while
requiring v3==v1. The example compares exact JSON scope/head rather than converting
the string wire version into a different numeric type. No assertion, budget,
permission or compiler lint was disabled.

The verifier bounds and checks complete uncompressed parts and closed XML; it
does not claim every ZIP physical-layout detail, such as rejecting an otherwise
valid archive prefix. Source-only peer review found no concrete blocker in the
inspected archive/plain/parts slice; it was not an execution or global review.

Continue with the **actual upstream editor/canvas in the shared MCPApp host**:
manual edit/snapshot/ACK, person and agent on the same canonical document,
read-only historical export, freeze/owner/document retirement, browser input and
managed assets/pins. Rich DOCX and PDF remain unsupported. Only after those
contracts are real should catalogue/native registration be considered. Native
keyboard, accessibility/orientation, explicit export and full user recovery
remain separate acceptance. This foundation alone does not close #502/#180.
