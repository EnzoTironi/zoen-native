# Managed WordCraft: first headless slice

Tracks [#502](https://github.com/EnzoTironi/zoen-native/issues/502), under ArtCraft
[#180](https://github.com/EnzoTironi/zoen-native/issues/180). This does not register
an available App, a native editor, or a PDF exporter.

The adapter uses the real pinned WordCraft Session and DOCX writer. Its binary
framing is the existing Deck/Grid ABI: little-endian 4-byte metadata length,
bounded JSON, then raw document bytes. The response has the same framing and
`{ok, metadata}` or `{ok:false,error}`. No paths, base64, host storage or accounts.

`open` carries the existing `Scope` and `Head`. The single command is
`{command:"text.replace",params:{text}}`, under the existing `Mutation` envelope.
It translates to upstream `select.all` and raw `text.insert`; it is not a generic
command dispatcher. Human and agent callers follow the same proposal boundary.
The accepted document stays unchanged until an exact scope/mutation/next-version/
hash ACK. Retries in the same bridge return the same proposal; the canonical host
still owns authorization, durable idempotence, CAS, FileDoc and cold recovery.
The standalone process emits a proposal and cannot commit it. The existing Zoen
worker resolver does not yet advertise WordCraft.

## Deliberately narrow DOCX profile

Only plain-text packages produced by this pinned writer are editable. The ZIP is
bounded to 512 KiB compressed, 32 entries, 1 MiB per entry and 4 MiB total actual
inflated bytes. Names cannot traverse; duplicate entries are refused. Count and
declared sizes are checked before inflation; actual reads are bounded too. Text
is at most 32 KiB UTF-8; LF and tab are supported, other controls are refused.

The upstream DOCX reader is lenient and can retain macros and opaque parts. This
adapter compares **every uncompressed package part** with the actual writer's
`Document::from_text` package. Rich formatting, metadata, images, macros, extra
parts and ignored XML are refused, including benign imports outside this profile.
Opening never replaces original bytes with a best-effort reserialization.
After the actual Session edit, plain text is encoded into the same declared
plain profile; Session editing metadata is not part of this format. Export of an
accepted head returns its exact original DOCX bytes, including before ACK.

## Reproduction and evidence boundaries

Use a separate clean checkout at the existing pin:
`d1b0bfc3f9dd83126b162a42b667facecb003b7f`. Run
`python3 miniapps/wordcraft-ui/prepare-engine.py --upstream ABS_OWNED_CHECKOUT`.
This copies sources and records every input SHA; it neither builds nor downloads.
Run `python3 -m unittest discover -s miniapps/wordcraft-ui/test -p '*_test.py'` for
recipe provenance/refusal checks. In the prepared upstream workspace, genuine
Cargo resolution must update Cargo.lock before a locked source-guarded build.
The package to qualify is `wordcraft-zoen-host` (Clippy all targets, then lib
tests and an actual binary frame run). `--seed` emits the actual upstream writer's
blank DOCX. Never substitute a fabricated ZIP for an executed writer fixture.

Official WordCraft 0.9.0 has a real browser WASM release. Its release commit is
`2e6db0bd88795e80375176c01665cff7dbe2b057`; the selected pin differs only in README
and `.coderabbit.yaml`. Inspected release ZIP SHA-256:
`710d2f4dd95ac6234a043d15d8e82232d20330199dbdbc94159324eeb02631c1`.
Actual WASM: 43,967,642 bytes, SHA-256
`fe8e18258a33eebd4b32d311e1651ebd817bae18469599a6f16feb3403389ad5`.
This proves availability, not execution or a managed package. That release is
not copied into Zoen resources. Its size and full fonts/assets need a managed
package audit. The engine's wasm32 clock uses js-sys; browser WASM is not evidence
of WASIp2 support. No WASI or native keyboard claim follows from host tests.

Remaining vertical acceptance: real managed browser/manual editing, canonical
binary host registration and permission/CAS recovery, agent action approval,
native open/edit/save/cold recovery/export, accessibility and orientation. Keep
#502 and #180 open until their actual journey criteria pass.
