# Inspect a local Agent before starting its host

`agent-inspect` reads the existing independently keyed Agent account through the
normal Client and authenticated relay session:

```sh
zoen --home AGENT_HOME agent-inspect --timeout 30000
```

The home must already contain its account database and unlockable vault. A Person
account, missing account, unfinished signup or locally unregistered Agent refuses
inspection. The command takes the same exclusive local home lock as `agent-host`
and enrollment, so it refuses while another cooperating process holds that home.
It does not create an identity, import keys, configure providers or acquire new
authority. `--offline`, `--for`, positional arguments and unknown flags are refused;
the timeout must be positive. Sync and the relay State read share its deadline;
an unacknowledged State read fails without retry or partial output.

After normal sync settles, inspection requests the Agent's own relay State. It
returns one JSON document with schema `zoen.agent.inspect.v1`: current Agent,
owner and device IDs; status, kind and definition version; exact returned cursors;
current projected Spaces and timeline locators, sequence numbers and entry kinds;
its requests' statuses, artifact IDs/versions and located source IDs; and the
existing approved-source inventory. `replyExists` uses the normal host's own-reply
deduplication rule. It does not mean a tool executed or a result was accepted.

The output excludes names, message text, request titles/details, tool arguments or
results, persona/rules, credentials, provider environment and journal contents.
Failures use closed diagnostic codes, without copying raw relay errors. A source
ID that cannot be located through current timelines and approved-source discovery
is `null`; the coverage object counts these requests explicitly. Legacy requests
can also have no canonical tool source.

This is the current local projection, assembled by separate reads. The coverage
object explicitly records that it is not atomic, does not fetch older history
pages and does not cover the whole relay history. Missing cursors remain `null`;
inspection never seeds them. An empty result must not be interpreted as a global
clean queue, a host allowlist or a permission to resume historical work.

Normal synchronization can receive and persist events and maintain its existing
session/key packages. Inspection does not mark messages read, advance Agent
cursors, call Gate/Brain/tools, change definitions/grants or touch `agent-ready`.
The CLI stops synchronization on success and failure before releasing its lock.
It does not replace normal host admission or provide a task/source/artifact filter.

Refs #175 and #174. Actual host readiness, one reviewed fresh-artifact task and
accepted-effect recovery are separate runtime acceptance steps.
