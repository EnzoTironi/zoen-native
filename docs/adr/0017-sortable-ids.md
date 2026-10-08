# ADR 0017: Sortable ids (ULIDs)

Status: accepted (S2)

## Context
Spaces, items, grants, requests and local invite grants were identified by
`prefix_` plus 32 hex characters of pure randomness (`new_id`). That made every
id unique and opaque, but gave listing and sharding nothing to sort on without a
separate `created_at`. Event `client_id`s already used ULIDs so the outbox and
idempotency keys sorted by time. The plan (and the 1B-users design) call for the
same shape everywhere that is not a public key.

## Decision
- `new_id(prefix)` returns `prefix_` plus a lowercase Crockford-base32 ULID
  (26 characters): 48 bits of milliseconds and 80 bits of randomness.
- Within one process, `new_ulid` is monotonic: if the clock has not moved, or has
  stepped back, the previous value is incremented. Two ids made a microsecond
  apart on the same device always sort in creation order.
- `id_time_ms(id)` recovers the millisecond for any ULID-shaped id, and returns
  `None` for the old random-hex form and for anything else.
- Ids that are also a capability (local invite grant ids that appear in a link)
  keep using `new_secret_id`, which is still 128 random bits in hex: a ULID's
  timestamp would make the leading characters guessable.
- Identity and device ids stay Ed25519 public keys (hex). Space `seq` stays the
  only counter, owned by that Space's shard.
- Existing ids are left alone. Nothing parses the body of an id except
  `id_time_ms`, so old `sp_<32 hex>` rows keep working forever next to new
  `sp_<26 ulid>` ones.

## Consequences
- A fresh Space, item or request id sorts next to ones made around the same
  time, across devices to the millisecond and on one device strictly.
- FDB key layout and Postgres indexes do not change: they already key by the
  id string; lexicographic order of ULIDs is chronological.
- Invite codes on the relay stay 10 random Crockford characters (they are short
  and secret). Blob addresses stay the sha256 of ciphertext.

## At 1B users
- 80 bits of randomness per millisecond gives about 2^40 ids before a birthday
  collision is likely inside one ms; far above any single-process rate.
- Sharding by id prefix (first few ULID characters = time buckets) is possible
  later for cold storage without remapping; Space ownership (S4) stays the hot
  path and still keys on the whole Space id.
- Clients never ask the relay for an id: every id is born on a device or as a
  content hash, so there is no global allocator to scale.
