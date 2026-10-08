# ADR 0009: Per-Space logs and pub/sub fan-out, not per-device inboxes

Status: accepted.

## Context
Messaging systems pick between writing each message once to a conversation log, or once per
recipient device into inboxes. Inboxes make reconnect cheap; logs make big groups cheap.

## Decision
- A message is one append to its Space's log in FDB.
- Online devices receive it through the NATS core subject `sp.<space>`, which every edge with
  a member of that Space connected subscribes to. Fan-out cost is O(edges interested).
- Offline devices catch up with per-Space cursors. To avoid O(spaces) round trips on
  reconnect, sync answers "which of my Spaces moved past my cursors" from `spaces_of` plus
  heads in one read batch.
- The only per-device state is small: push decisions (via `zoen-push`), MLS welcomes and key
  packages, and optional read cursors.
- Large Spaces use the tiers in system-design.md (pull for communities).

## At 1B users
20B messages/day become 20B appends, not 100B inbox writes: a 5x cut in write volume and
storage. Reconnect reads about 100 head keys per device (the average Space count), so a
reconnect storm of 16M devices in a minute is 27M point reads/s spread across cells, served
by FDB's read path (about 1 ms per batched read).
