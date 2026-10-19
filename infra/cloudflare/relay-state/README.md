# Cloudflare relay storage

This private Worker implements a persistent storage slice of the existing Rust
relay. It is not a public Zoen relay and does not implement the protocol-4
handshake, identity enrollment, provider login or encrypted sync. Production HTTP
requests return 404; workers.dev, preview URLs and custom domains are disabled.
Trusted Workers can call it through a service binding named `RELAY_STATE`.

The separate [closed-group adapter](../closed-groups.md) adds verified Rust/WASM
admission and atomic projection/invite effects in its own private namespace. It
shares the bounded SQL helpers, exposes no raw append RPC and has actual remote
synthetic proof. It does not make this storage-only service a public relay.

## Implemented contract

One SQLite Durable Object belongs to each Space. `appendAdmitted` accepts an event
already admitted by Rust, the observed head and Rust's next opaque admission-state
projection. It writes event bytes, author/client-id dedupe, head and projection in
one SQLite transaction. A competing append receives `conflict` and must reload
facts and repeat Rust admission. A lost-response retry receives the original
event, including if the retry changed its bytes, matching
`crates/zoen-relay/src/log/fdb.rs` rather than inventing a new dedupe policy.

The first sequence is zero. Sequence cursors stay canonical decimal UInt64 strings;
fixed-width text keys preserve order above JavaScript's exact-number range. The
storage object verifies predecessor/head consistency, bounds entries and opaque
projections to 1 MiB each, and bounds reads to 256 entries/4 MiB. The inclusive read
cursor matches `LogStore::read(space, from, limit)`.

`snapshot` returns the head and its corresponding opaque projection atomically.
Rust remains responsible for envelope/device signatures, canonical protobuf bytes,
chain hashes, current membership, privacy, MLS epochs, invitation consumption and
revocation fences. The projection must encode the resulting membership/MLS facts;
it is not a client-controlled event or a substitute for those checks. No public
request can call `appendAdmitted` directly.

The next adapter must reuse `log/admission.rs`, `roda-log` and `roda-proto` in Rust
compiled for Workers/WASM. It must compare the exact observed head when committing
its effects, derive duplicate delivery audiences from the current projection, and
preserve directory/device revocation fences before enabling public traffic. The
current native `zoen-relay` still opens PostgreSQL and FoundationDB unconditionally
in `src/lib.rs::build`; publishing this Worker does not change that behavior.

## Deployment and verification

Install the shared tooling with `npm ci` in `infra/cloudflare`, then run:

```sh
npm test --workspace relay-state
npm run check --workspace relay-state
node node_modules/wrangler/bin/wrangler.js deploy --config relay-state/wrangler.jsonc
```

The local tests use the same workerd/Miniflare version as the pinned Wrangler.
They cover competing appends, durable replay after runtime restart, author-scoped
dedupe, exact original bytes, atomic projection/head recovery, UInt64 boundaries,
bounded reads and mutation refusal. The HTTP test bridge and high-sequence seeder
exist only in `test/fixture.mjs`, which is absent from the production deployment.

An explicit remote QA command writes two small synthetic storage records into a
fresh `qa-cloudflare-state-*` Space through an authenticated Wrangler service
binding preview. It performs a second client session to check exact replay and
stored state. It does not prove a forced restart of the remote Durable Object:

```sh
ZOEN_CF_REMOTE_STATE_QA=1 npm run test:remote --workspace relay-state
```

On 19 October 2026, the private deployment
`03f7a50b-73bd-4e50-a9cd-5dfb1018ad3c` passed that remote check using
`qa-cloudflare-state-e6a9398b-3910-41b4-819f-538e8bee6c10` (two entries, five event
bytes). The four local workerd tests also passed. These results establish this
storage contract; they do not accept a public relay or login journey.

## Remaining Cloudflare migration

| Current dependency | Cloudflare target | Status |
| --- | --- | --- |
| FoundationDB per-Space transaction | SQLite Durable Object per Space | Storage and private closed-group Rust admission implemented; complete MLS projection and public `LogStore` integration remain |
| PostgreSQL directory, provider proofs, key-package claims, device revocations | Directory Durable Objects with SQLite transactions | Private Directory/provider/key-package adapter implemented; cross-Space integration, public provider flows and recovery remain |
| S3-compatible encrypted blobs and backups | Private R2 | [Private encrypted media](MEDIA.md) passes local differential and bounded real R2 QA; public integration, transfers and backup recovery remain |
| NATS fanout and native socket sessions | Durable Object WebSockets | Protocol/authentication and ordered fanout adapter remain |
| Native axum/Tokio process | Worker using shared Rust/WASM validation | Native Linux dependencies prevent deploying the existing process unchanged as a Worker |

Read-only account inspection found no Containers and no Zoen D1 database. The
existing OAuth integration at `auth.tryzoen.com` is a separate Worker; neither its
routes nor the four relay DNS aliases have been changed.

[Container disks are ephemeral](https://developers.cloudflare.com/containers/faq/)
and [snapshots are immutable, image-specific and expire after 30 days](https://developers.cloudflare.com/containers/guides/snapshots/).
[R2 FUSE does not provide full POSIX semantics](https://developers.cloudflare.com/containers/examples/r2-fuse-mount/),
so mounting PostgreSQL/FoundationDB data directories there is not an accepted
durability replacement. [Hyperdrive connects to an existing external database](https://developers.cloudflare.com/hyperdrive/get-started/)
and does not make those dependencies Cloudflare-hosted.
[SQLite Durable Objects provide transactional, strongly consistent storage and PITR](https://developers.cloudflare.com/durable-objects/api/sqlite-storage-api/),
which is the persistent foundation selected here.
