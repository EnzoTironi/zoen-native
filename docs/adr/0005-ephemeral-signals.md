# ADR 0005: Typing, status and presence are ephemeral

Status: accepted (milestone 1)

## Decision
Typing, "what I'm doing" (processing, building, in a call) and online presence travel as
`Ephemeral`/`Presence` frames. The relay checks membership and forwards them to members'
online devices; nothing is written to Postgres or to the device's log. Typing expires on the
receiver after 6 s without a refresh, so a lost "stopped" can't leave a ghost.
The journey test `typing_reaches_the_other_person_and_is_never_stored` counts events before
and after to prove it.

Presence is only shared with people you share a Space with.

## Not yet
Read receipts (`EphemeralKind::Read`) exist in the protocol; the UI doesn't send them.

## At 1B users
Typing and presence are the noisiest traffic and never touch storage. Typing is throttled
to one frame per 3 s per typing user, about 5% of online users type at once, which is 8M
users and 2.7M frames/s fanned out through the Space bus (ADR 0009). Presence is sent only to
co-members who are online and only on transitions, never as periodic heartbeats.
