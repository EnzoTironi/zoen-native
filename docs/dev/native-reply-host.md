# Native conversation host

`zoen-reply-host` runs the paid native conversation consumer as a dedicated
certified device. Its own MLS leaf uses the agent identity's canonical persona,
rules and controls. The existing SQL/FDB financial authority, encrypted native
capsule and independent continuity anchor own durable state.

The operator configures the host; product clients and model tools supply bounded
conversation/run locators or an already signed owner budget. The existing
owner-approved agent enrollment remains canonical. Public service provisioning,
distribution, general tools/approvals and iOS service integration remain in #175.

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

For a service that owns the executor lifecycle, set `supervised: true`, leave
`device` empty, and provide the existing absolute `ZOEN_REPLY_HOST_BIN` and
`ZOEN_REPLY_HOST_CONFIG` paths to `zoen agent-host`. The operator provisions infrastructure credentials and the private relay registry
once. Set that registry to `{"live":true,"devices":[]}`, select it through the relay's
existing `ZOEN_MANAGED_DEVICE_CONFIG`, and set `admission_config` in the executor
config to the same absolute file. Its directory must be private and separate from
the executor home. The directory/file remain outside SQL/FDB restores.

The executor appends only its currently certified principal and actual deployment
binding after successful native initialization. The relay adopts additions without
restart. No person creating an agent edits a principal list or chooses a device.

After verifying the enrolled agent's current state, the normal `agent-host` automatically
prepares retained device material through a private bounded pipe, certifies that
device using the agent sponsor, initializes its original custody/continuity and
starts the executor. It publishes readiness only after actual worker startup.
The sponsor publishes only a deterministic public device certificate proof,
never an IdentityBox, account root, agreement secret, profile key or MLS copy.
Unknown acknowledgements retry the same certificate bytes through the existing
relay admission transaction. Registration uses the existing atomic FileVault writer
and a shared operator-directory lock; an uncertain save resumes the same record. Revoked targets remain refused by that transaction.
The new device resolves from retained originals; recovery never reseeds it.

The configured standard host supervises the durable executor and the same signed
owner-budget control channel. It never constructs `ModelBrain`/`CodexBrain`, starts
editor workers, or seeds the old host's content cursors. Missing/relative/empty
executor paths, failed enrollment, and non-supervised configurations fail before
readiness; they never fall back to an unaccounted local model. `--source` uses
the same durable executor for one exact owner message, preserving other work and
scan cursors. See [selected-source hosting](agent-source-hosting.md).

`agent-controls` remains available for control-only services and separately
supervised executors; with `supervised: true` it uses the same lifecycle. With
neither executor environment variable present, existing local development hosts
and their explicitly selected providers remain available. That development path
does not inherit the durable financial authority described here.

A separately supervised executor remains supported with `supervised: false`
(the default). Its existing explicit setup is:

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
{"command":"search_history","run":"<original run>","arguments":{"query":"earlier decision","limit":10}}
{"command":"advance","run":"<original run>"}
```

The first example abbreviates the typed signed policy; the full canonical object
is required. Discovery returns bounded catch-up/completion flags and run IDs.
Pending returns a bounded cursor page. Advance reports `catching_up`, `held`,
`review`, `recovery`, `output` or `completed`, including actual delivery state. Financial
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


## Controls while the worker runs

On a Unix host, use `zoen-reply-host --config /private/config.json control`
with the same bounded JSON command on stdin. This connects to the running
`watch` process; it does not open custody, boot another FDB network or acquire
its home lock. `execute` remains the cold single-command mode and refuses a
concurrent owner. No broker payload becomes a control command.

The host home must be a real private directory (`chmod 700`). The socket is
mode 600 in a mode-700 ephemeral directory under `/tmp`, derived from the
canonical home and its filesystem identity. Routing is independent of subprocess
environment variables. This keeps macOS socket paths short
without moving any custody, database, vault or continuity state. A stopped
process's socket can be reclaimed under the home lock; symlinks, ordinary files,
foreign directories and active listeners are refused. Cleanup only unlinks the
same socket inode created by this process.

Requests are limited to 16 KiB, with a two-second input deadline, and responses
to 64 KiB with a two-second write deadline. Clients wait up to 150 seconds for
the existing bounded operation. One request is handled between scheduler cycles;
there is no second executor or concurrent native-device owner. An acknowledged
owner-policy installation reschedules deferred work, whose source, current
budget, lifecycle and publication authority are all checked again. Lost replies
retain the original installation and can be reconciled through exact replay.

The live-policy integration test disables spending, resumes the citizen, proves
that no new paid attempt starts, then installs the next signed version and gets
one answer in the same running process. The normal `agent-controls` companion can route account-budget controls through
this host with `ZOEN_REPLY_HOST_BIN` and `ZOEN_REPLY_HOST_CONFIG`. It launches
`owner-control`, with a cleared environment, into the same private live channel.
The companion retains its own certified primary device and transports only typed
owner inquiries/submissions and authenticated reports; it does not run models.
Exact image-request inquiries continue through their existing image host.

Account-budget quotes use the configured conversation price profile and finite
maximum reservation, with an explicit service tag. No image dimensions or image
provider configuration is fabricated. Historical image options omit the new tag
to preserve their original authenticated bytes. The iOS Core budget projection
uses generic profile/service fields; revision, limits and explicit confirmation
still belong to the existing Core. UI/device acceptance remains separate.
Windows live controls require a platform transport; its ordinary worker remains
available without this Unix channel.


Budget controls are authenticated opaque metadata for native conversation
extraction. Exact inquiry/submission signatures and report-original bindings
are verified, as are the historical writers and actual MLS opening/own echo.
The existing signed clear-envelope profile-key exchange is likewise non-content;
its key boxes remain encrypted per recipient. These controls preserve an existing
readable baseline but cannot open one, change grants,
enter model text or authorize an execution. Unknown and cross-scope controls
remain unsupported. Discovery cursor version 2 performs one bounded re-scan of
verified history to recover messages skipped by the former classification;
invalid legacy cursors still fail closed. The existing immutable trigger/run
index and paid-result receipts continue to prevent a second inference/charge.

## Supervised lifetime

`prepare-managed` is a service startup action with a closed `{agent,relay_url}`
request; both must exactly match the private operator config before any enrollment
write. It is not a model/conversation command. In external mode it returns only
`external` and leaves the worker untouched. In supervised mode its private pipe
returns the retained pairing code to the agent sponsor. The interactive `provision`
command keeps its terminal guard.

`supervised-watch` uses the same single custody owner and worker as `watch`.
Its stdin is a lifetime pipe, not a command channel: any byte, EOF or error asks
it to retire between durable ticks. Thus killing the parent also closes the pipe,
without depending on parent destructors. Normal termination closes the pipe and
waits for actual child exit; startup/read errors and early child exit fail the
control service. Existing native locks and financial/source fences remain intact.

## Live operator admission

Live mode is explicitly single-host, including an initially empty registry. The
relay pins its actual source cell at configuration time. A changed file is read
only from the original operator-selected private path, with byte/entry bounds and
stable-file checks. A complete candidate is validated before one atomic map swap;
all registry clones share the result. Unchanged files avoid reparsing.

Live changes may add principals; they cannot delete/reassign existing devices,
change a deployment binding or relocate its anchor. A refused/corrupt candidate
keeps all prior managed classifications, including ordinary-wire downgrade
protection. An unadmitted device cannot obtain a managed Runtime binding.
Revocation remains the canonical directory/owner control, not deleting JSON.
Cold startup rejects invalid configuration rather than starting an empty registry.
Static mode remains the default for existing deployments.

The configured limit remains 256 principals per relay registry. This local
integration proves live enrollment and recovery; it is not scale qualification or
a public multi-tenant control plane. Identity enrollment, current certificates,
source fencing, SQL/FDB continuity, financial reservation and owner grants remain
independent requirements for actual effects.

## Conversation history tool port

`search_history` returns `{"status":"history","page":...}` using the existing
Core `MessageSearchPage`: exact original message references, source conversation,
canonical authors, excerpts and an optional opaque next cursor. Arguments use the
existing closed conversation-search schema (`query`, `cursor`, `limit`, at most
20 hits). The model/caller cannot select an actor, source, conversation or grant
inside the arguments. The run must belong to this host's configured principal.

Each read catches up the source, restores the existing certified device capsule,
and reuses Client's native source authority and search index. Its temporary view
is consumed; rebuilding the derived index changes no retained image, MLS leaf,
outbox, financial policy or provider attempt. The opaque native cursor binds the
original intent and actual source frontier, so cold pages remain coherent and a
source change requires restarting the search. Errors never become empty results.

Current actor/definition, source head, grant, device fences and expiry are checked
before returning data, including after index computation. Pausing, revoking,
replacing the source grant or changing the definition refuses the old read.
Reads perform no model call, financial reservation or publication.

Configured canonical agents offer this search directly to the model. The
consumer can read up to three pages/queries before a final answer, sequentially
or in the same model proposal, with up to
20 hits per read. Each model request has its own immutable run/attempt, current
budget and one-shot admission. Tool results retain their actual message IDs and
roles. Mixed text/tool proposals are preserved; ordered final text segments are
joined literally before outbound review and publication. The final request preserves the tool history but disables new proposals;
a provider that ignores this cannot trigger another tool or model attempt.
Historical one-search/text-only records keep their original contracts.

Every continuation binds the exact parent capture, original message, principal
and previous transcript. Cold retries recover the same chain through the existing
native activation/checkpoint, evidence and financial journals. Parent wakes
hand off to the retained child; the final authenticated receipt also releases
ancestor image references through the normal receipt-based collection rules.

For the new turn format, a later unrelated message does not require resubmitting
the question. Before another inference or publication, the native index rereads
the retained pages under the current grant and compares their actual hits and
pagination boundaries. Only its own previously verified cursors are translated
through those fresh pages. The direct search API keeps its strict source-bound
cursor contract. Changed/deleted results, replaced/revoked grants, actor or
persona changes refuse stale execution. No stale result becomes a read permit.

Canceling any ancestor blocks its descendants and removes their wakes from the
runnable page without refunding already incurred calls. Observing retained cost
or an accepted historical receipt remains separate from productive execution.

General tools/owner-reviewed effects, consumer onboarding and product acceptance
remain open in #175. Apps/editors remain paused.
