# Managed agent service

`zoen-reply-host --config /private/service.json manage` owns one runtime,
financial namespace, capsule custody and independent continuity writer. It
publishes sealed enrollment offers and installs separately certified native
devices after owner consent. It never copies the owner's account into a host.

The iOS Create agent sheet uses the same native flow through scoped JSON commands.
`registered` means installation evidence, not live presence or iPhone acceptance.

The configuration extends the existing private reply-host configuration:

```json
{
  "agent": "",
  "device": "",
  "supervised": false,
  "managed_service": {
    "id": "<64 lowercase hex characters>",
    "definition": "<canonical bootstrap User definition JSON>",
    "zoen_binary": "/absolute/path/to/zoen"
  }
}
```

Retain the normal namespace, source, relay, price, gate, vault-key references,
absolute home/continuity paths and a live operator `admission_config`. The JSON
file is private (0600), and its home is private (0700). Provider/database secrets
stay in the existing `FileVault`; configuration contains their locators. The
operator must inject the existing capsule key at
`RuntimeAuthority::native_capsule_vault_key(namespace, key_version)`. A missing
or changed key never creates a replacement.

For a fresh paired deployment only, run:

```sh
zoen-reply-host --config /private/service.json initialize-service
zoen-reply-host --config /private/service.json manage
```

Initialization uses the existing SQL/FDB/independent-anchor bootstrap guards.
Ordinary startup opens and verifies retained continuity. Neither command resets
used stores. Configuration is retained exactly: changes require an explicit
migration, not deletion of its retained receipt. Preserve home, custody, SQL,
FDB and the independent anchor according to their existing recovery contracts.

Each owner/operation has separate `account/` and `executor/` homes. The normal
`agent-enroll-wait` command confirms registration and the reviewed definition.
The service then certifies the dedicated device, initializes its native capsule,
publishes key packages, updates operator admission, proves the actual relay route
and starts the existing worker. Only then is installation reported. Budget and
conversation grants still require the normal owner authorization.

The control companion uses `supervised: false`, so it connects to this service's
existing worker instead of opening another financial executor. Enrollment/control
companions hold a parent pipe and retire when the service dies. Native workers
run on a Tokio `LocalSet`, respecting SQLite's thread-affinity requirements.
Credentials are loaded before recovering pending native intents; revoked devices
grant no effects. Unrecoverable history fails closed instead of being discarded.

The public queue keeps its existing limit of 16 outstanding requests per owner.
The service uses bounded keyset pages and at most 256 concurrent jobs/native
principals. These are declared operating limits, not evidence of production scale.
Active-request cancellation, cross-device request discovery, public deployment
and iOS product acceptance remain open. Closing a sheet retains its request.

The automated `managed_service_owner_consent_first_reply_and_cold_recovery`
journey exercises actual owner Core, WebSocket relay, PostgreSQL, FoundationDB,
service/control processes, encrypted enrollment, native key packages, a signed
budget and conversation grant. Its HTTP model fixture is deterministic; it does
not prove an external provider or iPhone UI. It verifies one response with the
owner closed and no second inference, charge or reply after process restart.

## Native onboarding bridge

`managed_agent_start` takes a display name; Core creates the handle and retains
one operation before dispatch. `managed_agent_status` replays that exact intent
when its ACK was lost. The projection contains no pairing code, sealed offer or
private key. `managed_agent_approve` binds the reviewed digest to the retained
owner/device request; ambiguous delivery is reconciled through its native receipt.

After registration the sheet reuses canonical owned-agent discovery and the
existing account budget sheet. No money is granted by creating the agent.
Explicit Chat invokes `managed_agent_chat`: canonical installation is checked
under the current Net lease, then the private owned-agent DM gets its first
Listen grant. Retries preserve existing grants, including revocation.
`managed_agent_complete` retains a completion receipt before retiring the exact
intent, permitting cold replay without clearing a subsequent operation.

An unapproved expired/rejected offer can be explicitly retired through
`managed_agent_discard`. Any retained approval blocks retirement, even if its
ACK is unknown. This does not cancel an active request or revoke an agent.
JS owns only display state: focus/background/account retirement drops pending
callbacks and polling; reopening reads the same native operation.
