# 0049: Agent enrollment requires owner device authorization

Status: proposed. Production rollout and the M3 agent runtime remain separate gates.

## Problem

An agent could register a profile with any registered identity as its `owner`. The
relay checked a foreign key, but never obtained the owner's consent. A registered
person could also replace their public profile with an Agent profile; the directory
JSON changed while the SQL `kind` and `owner` columns stayed unchanged.

## Decision

An Agent profile carries an optional `owner_proof` for compatibility with historical
JSON and protobuf profiles. New registrations require the proof. It contains the
owner's device id, its root-signed device certificate, and that device's signature of
`zoen-agent-owner-v1\0OWNER\0AGENT\0DEVICE`. Each id is a canonical Ed25519 public key.
Verification rejects weak keys, invalid certificates, self-ownership, and signatures
for another agent, owner, device, or domain.

In the registration transaction the relay checks that the owner is a registered
Person and that the authorizing device is enrolled under that owner, has the exact
certificate, and has not been revoked. The device row stays locked `FOR SHARE` until
registration commits. An Unlink that commits first makes enrollment fail; an Unlink
that follows a successful enrollment does not rewrite the agent's ownership.

The agent profile, its SQL ownership metadata, and its first device enroll together.
Existing profile updates retain the original kind, owner, and proof. Ordinary name,
bio, and handle edits remain possible after the original authorizing device is
revoked. Agent ownership does not depend on one device remaining enrolled forever.

An unsigned historical agent may add its first valid proof using an active agent
device. The SQL owner must stay unchanged and the owner device must be enrolled at
that transaction. Such an agent is omitted from directory discovery until it has a
valid proof. Directory reads also check the profile against its SQL id, kind and
owner, so a historical JSON mismatch cannot invent a different principal.

The core verifies remote Agent profiles before storing them and pins verified ownership.
Unsigned historical remote cache rows cannot establish that pin. On reload they are
omitted from agents and trust decisions and requested again when referenced by a log.
An authenticated directory correction may replace their false kind or owner before
the first valid ownership proof establishes a binding. Subsequent owner, proof, or
kind changes are refused, including after restart. Local identities held by this
device remain protected from directory replacement.
The native `authorize_agent` contract produces a proof from an unlocked owner device.
The optional `agent_owner_proof_v1` capability lets an agent runtime detect supporting
relays before it provisions an identity; the relay enforces authorization even when
a client announces no capabilities. Historical profiles without a proof preserve
their serialized bytes.

## Scope and remaining gates

Ownership authorization grants no Space membership, MLS key, Trust level, tool
permission, or spending budget. Those require their existing owner-approved events.
This change does not provide a model gateway, durable agent workers, provider billing,
key custody, or an ownership transfer protocol. Native UI enrollment, paid-provider
journeys, and production deployment are not proved by the backend service tests.

## At 1B users

The proof adds two Ed25519 verifications and bounded directory row reads when an
agent first registers or a legacy identity adds its first proof. Regular message
delivery does not execute these registration checks. Profile reads verify stored
proofs and have existing result limits. No capacity or global traffic claim is made;
deployment requires measured registration/profile load and a reader rollout plan.
