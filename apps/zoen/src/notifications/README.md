# Notification and link boundary

`NotificationProvider` runs inside `ClientProvider`. It reads OS permission without
prompting on foreground; only the explicit Settings action prompts. Native APNs
tokens are kept in memory, validated as variable-length hex and serialized through
the existing authenticated Rust `register_push` operation. Token rotations and
removals cannot overtake earlier writes. Account changes invalidate callbacks.

The current Rust operation is online-only and returns `Reply::Done`. Therefore
this client reports `acknowledged` only after that reply, `waiting` for a transport
failure, and never invents durable `queued` state or verified provider delivery.
Retries wait 10/20/40/80/160 seconds and then stop. A future durable registration
DTO must come from Rust/relay, including explicit provider availability. Closing
the app does not unregister its token; denied OS permission sends an empty token.

Remote registration is disabled unless the build explicitly supplies
`extra.zoenPush = { enabled: true, environment: 'sandbox' | 'production', topic }`.
The environment must match the signed entitlement; it is never inferred from
`__DEV__`. The CNG notifications plugin, valid signing and provider credentials
are separate requirements. No token or key is printed or returned by this UI.

Remote notifications carry only a wake. They never navigate, approve, execute,
or show untrusted remote message text. `decodeWake` bounds the input and keeps
the entire UInt64 cursor exact. JavaScript object input has already lost lexical
JSON integer-vs-float information; strict raw APNs type validation still belongs
at the native boundary. Rust sync remains responsible for account, membership,
log and decryption validation. Currently the running app refreshes its existing
Rust projection on a valid wake. Headless background delivery still needs a safe
native single-session sync entry point and TaskManager integration; starting a
second JS-owned engine is deliberately avoided.

Local notification content comes only from current authenticated snapshots after
sync, excludes muted/archived spaces and own messages, and uses deterministic
identifiers. Taps accept only the current device's strict local metadata and a
current member space. Cold-start notification responses are read once and stale
callbacks are discarded. A remote payload cannot supply an arbitrary URL.

`+native-intent` accepts documented `zoen://app`, `chat`, `join` and `agent` links
and HTTPS links on `tryzoen.com`. Capability invitations reach a preview screen
before joining. Browser `/a`, `/join` and `/app` routes use the same UI. Universal
link association/hosting still require the matching deployment configuration.

Boundary tests live in `src/core/notifications.test.ts` and `deep-links.test.ts`.
They prove input bounds, serialized rotations, acknowledgement timing, account
invalidation, bounded retries and safe navigation; they do not prove APNs delivery.
