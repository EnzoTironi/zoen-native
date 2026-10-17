# Disposable localhost QA device

This example supplies a synthetic primary device without building `zoen-cli` and its
agent runtime. It uses the normal Rust engine, relay registration, signatures, grants,
linked-device protocol and history transfer. It introduces no production bypass.

Start the existing local development relay, then run from the repository root:

```sh
CARGO_BUILD_JOBS=2 CARGO_PROFILE_DEV_DEBUG=0 CARGO_INCREMENTAL=0 \
  cargo run -p roda-ffi --example qa-device -- \
  --handle qa_ios_pair_1 --relay http://127.0.0.1:8787
```

Use a fresh `qa_` handle for each run. Both options are mandatory. The relay must be an
HTTP loopback address literal (`127.0.0.1` or `[::1]`), with no credentials, path, query
or fragment. Hostnames, LAN/public addresses and production relays are rejected.

The first JSON line has `event: "ready"` only after real registration and relay sync.
It includes the public account/device IDs. Send one canonical command per input line:

```json
{"op":"snapshot"}
{"op":"create_group","title":"QA history","member_ids":[]}
{"op":"send_message","space_id":"<returned group ID>","text":"QA durable history before linking"}
{"op":"link_code_check","code":"<code shown by the QA app>"}
{"op":"link_device","code":"<same code after checking both screens' digits>"}
{"op":"link_progress","device_id":"<device ID returned by link_device>"}
{"op":"send_history","device_id":"<linked device ID, once link_progress is complete>"}
```

Successful replies are `{"ok":true,"result":...}` and retain the core's existing wire
JSON. Replies report only existing canonical operations; delivery still needs normal
sync acknowledgement. The fixture accepts a small allowlist covering pairing, history,
conversations, messages, pages and miniapps. It excludes provider sign-in, protected
vault/recovery operations and key output. Requests are bounded to one MiB per line.

Send EOF to stop. SQLite uses `:memory:` and the SecretVault is volatile; on normal drop
the vault buffers are zeroized. The account's identity/device keys and local database
in this fixture intentionally disappear when this process ends. A linked QA app retains
its own transferred keys. Keep the fixture running while the app receives its history.
Local relay registration/test data may remain, so restarting with the same handle is
not account recovery. Never use this fixture for an account or history you need to keep.

After compiling once, run `target/debug/examples/qa-device` with the same flags directly
to reuse the binary while other build work proceeds.

Pair only the dedicated QA app/simulator. The fixture does not supply Google/Apple
credentials, provider proofs, real-user data or a completed provider-login verdict.
