# Host one explicit Agent source

Inspect an existing independent Agent before starting it. When the current
projection contains unrelated backlog or approved continuations, an operator can
select one existing signed text message:

```sh
zoen --home AGENT_HOME agent-inspect --timeout 15000
zoen --home AGENT_HOME agent-host --source MESSAGE_ID --for 240
```

The selector is optional. Without it, the normal host still processes its regular
backlog and approved continuations with relay-backed cursors. With it, `--for`
must be explicit and positive; IDs are exact, never prefixes. Unknown flags,
duplicate flags and malformed bounds fail before opening the home or configuring
providers. The same exclusive home lock protects both modes.

After normal strict synchronization, source mode reads the Agent's own relay
State and locates exactly one entry in the current local projection. It refuses
missing, ambiguous, deleted, empty, self-authored and non-message targets. A shared
App card is not a text task, even if its signed caption is nonempty. The Agent and
the author being served must remain members of that Space. It never fetches older
history or falls back to another task when the requested entry is unavailable.

The account, certified device, owner, configured relay, Space, message ID, author
and observed text digest are bound in memory. They are rechecked around dispatch,
tool reads/execution, approval staging and result delivery. This selector adds no
grant, artifact access, provider credentials or budget. The ordinary Runtime,
Gate, serving-member checks, canonical approvals, existing tool journal and
accepted-result recovery remain authoritative. The normal bounded history is
still context for the selected source; other entries are not dispatched as turns.

Source mode bypasses both broad approved-source discovery and new-message
processing. It never seeds or advances any Space cursor, skips old messages by
advancing a watermark, or marks a Space read. Finishing the selected source leaves
those cursors untouched. A later normal full host still handles its ordinary
backlog and uses its existing source-reply deduplication.

In local developer mode, a handled source is observed once per latest canonical Request
ID/status. Waiting for approval, Silence and Block are not repeated model tasks on
every wake. Only the selected source's canonical transition can resume it. An
unknown result or elapsed in-flight turn returns a fixed error and stops; it is
not automatically retried as a new model call. Existing Requests, checkpoints and
receipts remain available for explicit recovery. Timeout does not cancel an
already accepted server effect.

The host can stop early when its own reply is accepted and sync settles, reporting
`agent_host_source_reply_completed`. Otherwise it waits up to its normal `--for`
window and reports `agent_host_source_wait_elapsed`; elapsed time does not claim
completion. Startup/sync shutdown have their existing separate bounds. The fresh
`agent-ready` marker remains startup evidence only, not liveness, permission or a
claim that the whole backlog was processed.

Qualification uses signed synthetic Client projections and a certified loopback
transport. It covers refusal, scoped dispatch, canonical status transitions and
absence of cursor/read effects. It does not constitute real provider, WASI
execution or full iOS acceptance; those require separately authorized evidence.


## Configured durable service

With `ZOEN_REPLY_HOST_BIN` and `ZOEN_REPLY_HOST_CONFIG` configured, the same
invocation uses the supervised native executor and existing financial authority.
It never starts the local developer brain or editor workers. The sponsor binds
the chosen timeline message ID, conversation, author and plaintext digest from its
current authenticated projection. The executor independently resolves that exact
message from its own native history and current grant; the closed native profile
currently serves its owner. Unsupported sources fail without selecting another.

Only that source and its verified descendants run. Source synchronization applies
actual relay history without advancing discovery cursors. The existing immutable
trigger index retains the original run, capture, price and receipt across retries
and restarts. A later normal worker can still discover other messages. Historical
receipt evidence cannot authorize new effects; current actor, definition, source,
policy, cancellation and owner approval remain required.

Private operator IPC supports `discover_source` with a `source` object containing
`space`, `message`, `author` and `text_sha256`, and `advance_source` with the same
object plus `run`. These are locators, not grants. The standard CLI sends the
selector over its private supervision pipe; users do not configure IPC.
`agent_host_source_reply_completed` requires an accepted final response and a
successful executor exit. A `--for` expiration reports elapsed time instead.
Uncertain paid calls retain their original identity and recovery state; a retry
never creates another inference merely because the result is unknown.
