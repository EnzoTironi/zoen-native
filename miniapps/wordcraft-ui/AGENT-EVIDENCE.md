# Word factory and reviewed agent consumer

Continuation of #502/#180 over #532 `8f447da1`. This adds the normal consumer,
not a replacement engine, Store, Host, journal or native acceptance.

The standard App factory can create a real blank DOCX/FileDoc/card when the exact
configured browser package is present. `create_wordcraft` uses the existing
owner-reviewed original creation and deterministic seed/Item intent. The agent
gets `read_wordcraft` and `wordcraft_text_replace` only from its configured signed
worker and canonical artifact. The write is the real upstream `select.all` +
`text.insert` implementation behind the closed `text.replace` adapter. Actor,
owner, source, device, membership, grant, reviewed arguments and version/hash
remain checked by the existing native turns. Word mutations use `wordcmd` in
both execution and recovery; they cannot borrow Deck/Grid bindings.

The unchanged package03 is admitted by its actual JS/WASM/HTML pins and zero
network/permission resource contract. The bootstrap was regenerated from source
and is byte-identical: 624,158 bytes, SHA256
`a84383c0e5e568ea3d67ae4685ce5116a2b720805bb60f49ba73f56e8f108095`.
Its genuine input graph and notices are versioned in `resources/generated/`.
The default Word manifest has no runtime package and is unavailable. An explicit
manifest is a deployment attestation as for Deck/Grid, not proof a URL is live.
No route or service was installed. `appAvailable`/`nativeAccepted` in package03
remain unchanged. Replacement text is bounded to 16 KiB UTF-8 and the original
32 KiB serialized signed-argument limit, including JSON escapes.

## Actual scoped execution

External receipts: `/Users/enzotironi/.codex/builds/zoen-word-agent-operations-v547/`.
All Rust uses an owned private APFS clone of the idle Word cache, jobs=1, debug=0,
incremental=0, normal source guard, offline/locked and 8 GiB free-disk floor.
There was no v410 access, provider, service, SDK or device operation.

- `9f9b19f1`: Miniapps/Core/CLI all-target Clippy `-D warnings` PASS40.002s;
  Miniapps Word6 PASS, including the actual archive/profile/registry and closed
  command boundaries. Existing Word/binary Node25 PASS (transport fixtures,
  not a second Core grant or browser execution claim).
- `a4defc14`: Core Word10 PASS/0ignored and actual catalog1 PASS. Covers person
  factory/card/host saves, certified agent review, wrong-format authority,
  revoked grant, original CAS receipt, rollback and selected historical bytes.
  Core default/client-types all-target Clippy PASS26.822s/20.766s.
- Actual opt-in CLI `actual_word_create_approve_lost_result_edit_and_cold_reopen`
  PASS1/0ignored, body167.62s/guard245.300s. It loads the genuine signed WASIp2
  `a9200940b3ec522e645e5d342304267d041f43296129d44c3a0922f6f5bfbb2a`,
  uses a real certified synthetic agent/owner and controlled Gate/brain, stages
  creation without effects, applies exact owner approval, loses the creation
  result, cold-reopens SQLite/vault, recovers the original Item, reads it,
  obtains a second exact approval and executes actual `text.replace`. It cold
  reopens the accepted DOCX, then saves through the ordinary host CAS on the
  same Item and keeps the agent's exact v2 bytes as selected history after v3.
  No model inference, relay or browser port is substituted for Core persistence.
- Default build profile is separately executed: Miniapps Word6 PASS with the
  resource environment absent, proving no default catalog/factory enablement.
- Actual manifest producer5 negative executions PASS: wrong engine/hash/size,
  HTTP and symlink asset refuse without output. The positive manifest verifies
  the genuine sealed package03 assets, independently of Rust pins.

The body includes cold component JIT, not a startup latency guarantee. Per-call
5s/2-billion-fuel/256-MiB limits are unchanged. The CLI fixture exercises owner
signatures and real worker/Core/CAS with a controlled brain; it is not physical
input or an end-to-end browser-to-relay conversation journey.

The new manifest exposed a real build-input gap: changing only Word deployment
configuration reused the previous native source stamp (RED1, unchanged stamp).
Adding Word to the source fingerprint and guarded environment fixes that exact
cut: builder fixtures7 PASS; conservative artifact-scope13 PASS. Cargo roots,
selection and compiler flags do not change. The reviewed builder digest is
updated for those exact bytes. These tests use tiny real Cargo/exporter fixtures,
not the application/native build.

## Preserved failures and remaining product work

`test-01` compiler E0433 was a missing fixture `EntryKind` import; `test-02`
compiler E0277 was an unconverted JSON serialization error. No behavior executed
in either. `test-03` Core8PASS/2FAIL exposed fixture expectations: the old
foundation unconditionally required catalog unavailable, and the new test used
`person` where the existing wire actor kind is `human`. Those exact failures
remain retained; existing CAS, signature and revocation assertions were kept.
The final correction changes only those two tests, so Miniapps/CLI production
is byte-identical to its earlier Clippy qualification. No broad Core/workspace
or native result is claimed.

The accepted current/historical PDF operation now uses the real pinned layout
and writer; its missing-font RED, closed refusal, actual WASI calls, normal web
package and independent output inspection are recorded in
[PDF-EVIDENCE.md](PDF-EVIDENCE.md). Those results do not retag this earlier
package03/agent qualification or establish native export acceptance.
Rich DOCX imports/formatting are still outside plain-text-v1.
Deployment, actual browser-to-canonical-host integration, iPhone keyboard,
accessibility/orientation and native export/recovery acceptance remain open.
#502/#180 are not closed by these fixtures or this source publication.
