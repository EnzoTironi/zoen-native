# ADR 0014: Memory, files, skills and knowledge are Items in the Space log

Status: accepted, built after hooks (M2.6). Ideas drawn from public descriptions of TextQL's
ontology work and the Muse runtime-export incident; no text or code taken (those sources
carry no usable license).

## Decision
- **One source of truth.** Files, notes, skills, claims, indexes and role behaviour are Items
  (`ItemKind::{File, Note, Skill, Claim, Index, Behavior}`) with an optional `path`, living
  in the Space's signed, MLS-encrypted log. Versioning, authorship and citations are
  `(space, seq, hash)`; there is no git server. Folders are path prefixes, a view.
- **The Space is the access boundary**, because only a Space has a group key. Sensitive
  material goes in a child Space. Sharing one Item with a non-member uses a per-Item content
  key sealed (HPKE) to the grantee inside a `GrantIssued` event; revocation rotates the key
  for future versions, and the UI says plainly that what was read stays read.
- **Two memory layers.** Space memory, visible to every member including what the agent
  noted; personal memory in the user's Personal Space, read only by the user's devices and
  their own agents. Agents write back through the request flow with approval bound to the
  content hash; an agent write can never widen an audience, add members or issue grants.
- **Routing first.** Each Space has a small index Item that agents always load; everything
  else loads on trigger (index, then search, then references). This bounds tokens per turn.
- **Memory API.** `memory_search`, `memory_explain` (citations only for events the asker can
  decrypt now) and `forget` (tombstone plus deletion of every derived artefact, then
  re-consolidation without it).
- **Search v2.** FTS5 plus sqlite-vec (`int8[384]`, on-device embeddings), reciprocal rank
  fusion with recency and an index boost. Indexes are derived, rebuildable, never synced in
  plaintext, and dropped on forget, revoke or removal.
- **Agent runtime cache** per (agent, Space), encrypted under a key from the MLS exporter for
  the current epoch; removal makes it unreadable.
- **Export** is a signed bundle built on the device (`manifest.json`, files at their paths,
  `proofs/` with the backing signed events). Destinations are external actions that always
  need the owner's confirmation; no agent can start an export.
- **Muse lessons as rules.** The agent sandbox never sees host secrets or a filesystem root,
  can't enumerate its runtime, has no network unless granted, and connector writes always
  confirm.

## At 1B users
No new server tier: memory is ordinary log events (envelopes up to 100 KB, larger content
as blobs), so server cost is log storage already in the model. On the device a heavy user's
60k chunks cost about 23 MB of int8 vectors. The real cost is agent tokens: at 1B agent turns
a day, routing (about 4k input tokens) instead of history dumps (about 20k) saves about 1.6e13
tokens a day, which is why routing-first is a cost decision.
