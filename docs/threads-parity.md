# Respostas: sincronização e leitura

Thread replies use the existing one-level `ReplyRef` protocol. Replying to another
reply resolves to its root and preserves `also_in_chat`; thread-only replies do not
increase the conversation unread count, while broadcast replies appear in both views.

Native and web clients exchange `ThreadStateSet` events in the account's encrypted
Personal Space. `root` is the message's stable client ID, not a relay hash. Explicit
follow/unfollow overrides automatic following (author, participant or mention).
`read_up_to` is a monotonic per-conversation sequence watermark. Device-local browser
preferences and historical reply totals are not used as unread state. Personal Space
requests device enrollment just like a conversation when its MLS group is absent.

A failed durable write must not consume unread state or emit a thread-state frame.
Reading a quiet thread does not append another event. Native thread queries include
the root's follow/unread metadata; Apple refreshes the shared model after marking an
unread thread read. Android text replies preserve the broadcast checkbox even when
responding to a reply. Interface copy uses “Respostas” and “Acompanhar respostas”. The replies inbox remains
reachable with no unread replies or with only direct chats; web roots show unseen reply
and unread mention badges, with a navigable newest-first followed-replies list.

## Verification

- `cargo test --locked -j2 -p roda-ffi thread -- --nocapture`: passed, 5 tests.
  Includes mandatory incoming-send assertions, nested broadcast replies, whole-handle
  mention matching (including dot suffixes), and log-only replication to another
  device without copying local read metadata (also exercised with a direct chat).
  The replication test covers read,
  follow/unfollow, new replies after reading, monotonic stale-device reads and a
  failed append followed by retry.
- `cargo test --locked -j2 -p zoen-web-core --lib`: passed, 9 tests. Includes real
  native SQLite MLS / portable web-core encrypted Personal Space thread-state
  exchange, saved reload, durable-write failures, idempotence, watermark projection,
  explicit opt-out, broadcast replies, enrollment requests and mention boundaries.
- In `packages/zoen-ui`, `npm run typecheck:web`: passed;
  `npm run test:web`: passed, 14 tests. Controller tests exercise the follow/read/
  broadcast API and existing durable-save-before-send behavior.
- Mention regression: the added `@enzo.silva` versus `enzo` assertion failed before
  the suffix-boundary fix and passed afterward.
- `cargo test --locked -j2 -p roda-ffi`: passed, 318 tests at the merge checkpoint;
  the focused 5-test suite was rerun after the final activity-label/direct-chat and
  validated-reply helper changes.
- `xcrun swiftc -frontend -parse` on `MessageReplies.swift` and
  `ConversationsView.swift`: passed (syntax only, not an Apple build).
- Android XML parsing, Portuguese-copy and nested broadcast/inbox reachability source
  checks: passed (not an Android compile/runtime check).
- `cargo fmt --all --check` and `git diff --check`: passed.
- `cargo clippy --locked -j2 -p roda-ffi --lib -- -D warnings`: passed.
- `cargo clippy --locked -j2 -p roda-ffi -p zoen-web-core --lib -- -D warnings`:
  passed with both clients' feature union. The narrower web-only strict command fails
  on the unchanged MLS `leaf_keys` helper being unused under portable-only features;
  tracked separately in [issue #116](https://github.com/EnzoTironi/zoen-native/issues/116).
  No lint was suppressed to obtain the combined result.

## Remaining acceptance checks

These are source/core checks, not proof of a deployed product or a live paired
browser/native journey. The WASM target and wasm-bindgen executable are not installed
in this checkout. No WASM build, live paired browser session, Android build, Apple
build or visible UI capture was run. Local simulators/emulators are excluded by the
task constraints; native platform CI and live-device acceptance remain necessary.
The existing PR #97 publication association failed before any new commits could be
published, so its previous CI results do not verify these changes. No replacement PR
was created.
