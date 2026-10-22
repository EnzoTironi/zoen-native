# Browser cursor reconciliation contract

The browser and native client share `protocol::cursor_repair`. Its connection-local
bookkeeping owns ordered initial, gap/join and periodic `Sync` completions. Verified
logs and the existing encrypted snapshot/outbox remain the durable authority.
The existing Zoen policy is 60s periodic admission, 30s periodic completion budget,
and 60s initial verified-progress inactivity. These are not Signal measurements.

The browser's independent admission/watchdog timers surround the message serial
queue. Expiry retires the authenticated generation before requesting socket close;
a delayed `onclose`, write or IndexedDB completion cannot revive that generation.
Ready starts its budget outside the queue. Subsequent receive/reconcile clocks are
sampled at their actual serial Core boundary, before persistence; saved metadata
keeps absolute deadlines. Pong and UI/MLS maintenance do not renew those deadlines.
Old/malformed Core policy is refused before transport startup. Publish matched
main/worker/WASM outputs using the existing integrity-bound Web build.

## Opt-in real browser fixture

Use a dedicated target and fresh output directories. The exporter uses only a new
in-memory SQLite/MLS fixture and fixed test-only owner/device keys. It refuses an
existing output file. The runner verifies those synthetic public keys, serves only
owned loopback HTTP/WebSocket resources, starts a fresh headless Chrome profile,
and closes its own process group/server/socket handles. It never opens a retained
profile, authenticates a provider, starts a production relay, or builds/downloads
its own Rust inputs.

From the repository root, with an existing compatible toolchain, wasm-bindgen and
package dependencies:

```sh
cargo build --locked -p zoen-web-core --target wasm32-unknown-unknown -j 1
wasm-bindgen "$CARGO_TARGET_DIR/wasm32-unknown-unknown/debug/zoen_web_core.wasm" \
  --target web --out-dir "$fixture_dir/generated" --out-name zoen_web_core
ZOEN_BROWSER_CURSOR_FIXTURE_OUTPUT="$fixture_dir/fixture.json" \
  cargo test --locked -p zoen-web-core --lib \
  client::tests::cursor_repair::export_synthetic_cursor_fixture_for_real_browser_worker \
  -- --ignored --exact
node packages/zoen-ui/web/browser-tests/run-cursor.mjs \
  --fixture "$fixture_dir/fixture.json" --generated-root "$fixture_dir/generated" \
  --output "$fixture_dir/fresh-browser-attempt"
```

`CARGO_TARGET_DIR` and `fixture_dir` must be explicitly chosen owned paths. No
fixture key, signing payload, plaintext or raw protocol frame is printed. Failed
attempts retain their public result and bundle hashes in their own fresh directory.

The peer withholds the last native-encrypted event entirely until a later full
Sync on the same healthy socket. The test retains a real outgoing message save
from roughly 59s until 65s and delays one Pong by 1s within the unchanged heartbeat
budget, forcing a queued tick followed by a queued receive. It checks exactly one
periodic wire Sync, native MLS verification/decryption, duplicate-event admission,
then closes/reopens CoreWorker and BrowserVault in the same Chrome process. The
new bootstrap must use cursor 4 and replay the same pending application ciphertext;
its accepted remote event and local pending message must each remain unique.
Core polling-call count is observed separately from wire Sync effects. Unrelated
clear publications are not classified as the selected encrypted application.

Host tests cover expiry/stop, late persistence, old-generation replacement,
unsupported policy, tab suspension and the65→61 clock-order regression. The
browser fixture is not a production-relay authorization, paging, network-outage,
remote-revocation, physical-device or all-browser qualification. Native watchdog
regressions and browser/WASM results must be reported separately.
