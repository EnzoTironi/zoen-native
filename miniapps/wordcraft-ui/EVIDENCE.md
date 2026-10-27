# First worker qualification

Source: `b6d0b001b46cd43b1d545f5f4e3323cf5f4856a2`.
Prepared upstream: `9057de789030f5ed5b9dbd4eac35be7556191304`, based on the
pinned `d1b0bfc3f9dd83126b162a42b667facecb003b7f`. Genuine Cargo.lock changes
are only the 13-line host package entry; no dependency versions were updated.

Private source-guarded qualification (`wordcraft/check-04`):

- Clippy all targets, `-D warnings`, authored package pedantic/perf: PASS, 12.668s.
- Six actual engine/profile/ACK tests: PASS, zero ignored, 0.04s execution;
  341.529s guard including cold compilation.
- Actual native worker build: PASS, 3.539s.
- Eight actual framed binary calls: inspection, Session edit, cold DOCX reopen,
  exact retry, unknown-command/hash/scope/stale-base refusals: PASS.
- Source and provenance unchanged; no services or shared target used.
- Offline recipe tests: 3 PASS. Public clean-pin recipe replay matched all seven
  prepared host files and the source receipt byte for byte.

Result SHA-256: `ad4d4f430e45f9b9eeffffede1f0aeca3f95f10a1de524c95356ea70b0ac6038`.
Frame receipt: `346f84a7c9cb12e3cbd598331701e61d5749f42b07fb2663d1b7ba10ccafa52a`.
Native executable: `2ad54341d4d6d8e68316e4916837fdaa158fd7bfe85619ad88387e4bfa6f9c12`,
42,128,272 bytes (development profile, not a release size estimate).
Actual writer seed: `0a4363cf44e798c967e35b2ec7addce8697f4ab694844fba12c561fc0789310b`.
Actual proposal DOCX: `44fe788ffde11b77e29ddb1666294c4584a92c899e15c3eb902d62056c29c431`.

Earlier REDs remain retained: check-01 found the sha2 0.11 `LowerHex` API mismatch;
check-02 applied CLI pedantic to the whole pinned upstream and found 83 diagnostics
in unmodified geometry code; check-03 found 19 authored adapter diagnostics.
The API mismatch and authored diagnostics were fixed without lint suppressions.
Pedantic/perf are now package-scoped; this is not a claim that the entire pinned
upstream passes pedantic. The ZIP preflight also rejects identical duplicate
members before zip3 can deduplicate them.

These tests supply synthetic Scope/Head values. They prove engine bytes and
adapter behavior, **not** canonical host authorization, durable ACK-loss recovery,
App registration, managed browser/WASIp2 execution, iPhone keyboard or native
save/recovery/export. Those acceptance criteria remain open in #502/#180.

The later real visual editor and shared MCP browser qualifications are recorded
separately in [BROWSER-EVIDENCE.md](BROWSER-EVIDENCE.md); they do not change the
execution scope of the first native slice above.
