# Native conversation host

`zoen-reply-host` runs the paid native conversation consumer as a dedicated
certified device. Its own MLS leaf uses the agent identity's canonical persona,
rules and controls. The existing SQL/FDB financial authority, encrypted native
capsule and independent continuity anchor own durable state.

The operator configures the host; product clients and model tools supply bounded
conversation/run locators or an already signed owner budget. The iOS enrollment
and normal Brain/tool host integration remain separate work in #175.

## Enrollment and startup

Build with `scripts/cargo-local.sh build -p zoen-cli --bins`. Provide a private
0600 JSON configuration and an existing certified agent identity. Use distinct
absolute `home` and `continuity` directories. The configuration fields are:

- `namespace`, `agent`, `device`, `key_version`;
- `home`, `continuity`, optional `fdb_cluster_file`;
- `source_cell`, `relay_url`, `relay_key` for the actual relay-owned source;
- vault locators `postgres_key`, `evidence_key`, `chat_credential_key`;
- a complete trusted `chat_price` (`zoen_runtime::PriceProfile`);
- `gate`: `{"provider":"clef","account":"<32 hex characters>","credential_key":"<vault locator>"}`.

Secrets use the existing `FileVault` adapter in `home/vault`. The evidence key is
32 bytes. Config parsing rejects unknown fields, symlinks, public permissions,
oversized input and changed file identity. Errors omit configuration content.

1. Set `device` to an empty string and run
   `zoen-reply-host --config /absolute/private.json provision` in a trusted
   interactive terminal. This retains the original enrollment before displaying
   its pairing code. Resume that code after interruption.
2. Have the existing agent sponsor enroll that exact device using the normal
   device-link flow. Set `device` to the displayed certified device ID.
3. Run `zoen-reply-host --config /absolute/private.json initialize`. It establishes
   fresh shared-runtime continuity and publishes the native key packages. A
   repeated invocation independently verifies existing continuity and resumes
   the original initialization.
4. Register the shared deployment and dedicated principal through the relay's
   existing `ZOEN_MANAGED_DEVICE_CONFIG` `runtimes` entry. Retain the private
   continuity directory independently of database/vault backups.
5. Invite the enrolled device through normal conversation creation/membership,
   install the owner's exact signed period policy and deliver the conversation
   grant through the authenticated source.

Execution qualifies the actual relay route through
`qualify_shared_reply_output`; configuration alone does not admit a paid call.
HTTPS is the default chat endpoint policy. Automated local fixtures explicitly
choose `chat_endpoint_policy: "loopback_fixture"` (HTTP numeric loopback only).
`gate: {"provider":"offline"}` explicitly selects the existing offline floor;
omitting Gate configuration is an error. Clef uses its existing bounded transport
and `clef-flash`; credentials are read from the configured vault reference.

## Worker protocol

`execute` reads one JSON command from stdin and emits one JSON result. `serve`
retains the same authority and processes newline-delimited commands until EOF.
Each frame is bounded to 16 KiB and each result to 64 KiB. A supervising worker
owns waking and retries; serving does not poll or repeatedly evaluate held work.

```json
{"command":"policy_import","signed":{"policy":"...","device":"...","cert":"...","signature":"..."}}
{"command":"discover","space":"<conversation>"}
{"command":"pending","after":null}
{"command":"advance","run":"<original run>"}
```

The first example abbreviates the typed signed policy; the full canonical object
is required. Discovery returns bounded catch-up/completion flags and run IDs.
Pending returns a bounded cursor page. Advance reports `catching_up`, `held`,
`recovery`, `output` or `completed`, including actual delivery state. Financial
`null` means unobserved/absent, rather than released or zero cost.

Gate decisions run outside native leases/FDB transactions. Source, grant,
certificate, canonical citizen controls and native output authority are checked
after decisions and again before sending. The outbound audit binds the actual
retained response and remains in its encrypted publication. Pausing/revoking an
agent prevents new effects; paid originals and historical accepted/refused
receipts retain their separate recovery path. Retrying an original run never
creates a replacement inference or changes its frozen price/usage evidence.

The executable journey `journey_reply_runtime_device` uses real sponsor pairing,
separate MLS leaves, owner-signed policy, controlled HTTP, actual relay and owner
MLS recovery across cold processes. The runtime's `native-agent-context` cohort
covers policy races, retained results, threads and canonical definitions.

## Automatic worker

Run `zoen-reply-host --config /absolute/private.json watch` under the process
supervisor. It pages the existing relay membership index and the native wake
queue for its configured agent/device; each actual discovery/advance retains
all native and financial checks. No per-conversation operator list is needed.
Startup and periodic reconciliation recover missed notifications and cold work.

For prompt wakeups, set the relay operator environment
`ZOEN_WORKER_WAKE_NATS_URL` and put that URL in the host vault under the optional
`worker_bus_key` configuration locator. The wake-only publisher carries empty
payloads on pseudonymous `workers` subjects, has a bounded queue and drops hints
on pressure. It preserves the relay's existing topology and the single-host
restriction for managed devices. It does not use distributed relay fan-out or
answer presence pings. The worker ignores all broker payloads.

Each cycle pages up to 32 memberships and 16 wake records. Notifications coalesce
at a maximum four cycles per second; a two-second reconciliation timer remains
active when the broker disconnects or drops hints. Local monotonic cooldowns
avoid repeatedly evaluating held/recovery/denied work (60 seconds) and retry
transient work after two seconds. These are scheduling observations, never a
financial release or terminal-consumption record. Cooldowns are bounded to 4,096
entries; exceeding the declared profile fails closed. Losing process-local
cursors/cooldowns still recovers originals from the native source/wake queue.
The worker observes the canonical citizen control revision on each cycle. Owner
pause/resume uses the same reviewed Core command as iOS. A new revision clears
only scheduling cooldowns, so resume does not require a process restart or wait
for a prior 60-second deferral. Admission still checks the current lifecycle,
source, grants and budget. A revision rollback or disappearance stops the worker.

`pending` returns only the configured agent/device's work. `policy_import` checks
that device's current native directory and actual owner under the installation
transaction; a valid signature from another owner is refused without financial
writes. Exact replay acknowledges the retained policy, without resetting budget.
Invalid/conflicting policies report `policy_refused`, distinct from installation.

SIGINT/SIGTERM stop at a cycle boundary, finishing the current cycle before
closing custody. A kill still relies on existing durable admission, capture and
receipt recovery. Restore/binding failures stop the worker; unavailable storage
is retried. Live paid-provider validation, normal Brain/tool approval integration,
iOS product wiring, managed multi-host custody and production scale acceptance
remain open in #175/#170.
