# ADR 0027: End-to-end by default, and privacy only goes up

Status: accepted (M2)

## Context
ADR 0026 made end-to-end Spaces real, but every DM and group was still created relay-readable
(`Closed`). Enzo's mandate is the strongest privacy we can ship. M1 Spaces already exist on
devices and on the relay, and they must keep working: no silent change of what they are, no
lost messages, and a clear way to upgrade them.

## Decision

### Defaults
- **DMs and new groups are end-to-end.** `start_direct` and `create_group` (app and CLI) make
  `Privacy::EndToEnd` Spaces. `create_group_with(.., Closed)` and `zoen group --readable`
  still create a relay-readable group on purpose. Communities stay `Closed`: they're open to
  join, moderated and served by server-side tools, and their scale is past MLS's 1,000-member
  cap (plan D10).
- **Agents read only as declared members.** An agent sees an end-to-end Space only if it is
  in the roster and its runtime (`zoen-agentd`, M3) holds an MLS leaf with its own certified
  device key. Then it is added by the same reconcile as a person, from its key packages. The
  relay never gets a reading key. An agent in the roster with no runtime yet has no packages,
  so it is listed but reads nothing, and nobody's messages wait for it.

### Upgrading an M1 Space: `SpaceEncrypted`
- A new clear control event, `EventBody::SpaceEncrypted`. From that entry on, the Space is
  end-to-end. **There is no event for the other direction.**
- **The relay admits it** only in a `Closed` `Direct` or `Group` Space: from either person in
  a DM, and from an owner or admin in a group. It rewrites the Space's meta to
  `EndToEnd` in the same transaction, so from the next entry on it takes only ciphertext and
  the control set (ADR 0026). `Public` Spaces and communities are refused, and so is a
  second `SpaceEncrypted` ("already end-to-end").
- **Devices** set the Space end-to-end when they apply it, and show a system line ("End-to-end
  encryption is on. Only members can read new messages."). The author's device starts the
  group when the relay confirms the event, and its reconcile adds everyone listed (ADR 0026).
- **No downgrade, enforced twice.** The relay refuses a second `SpaceCreated` ("space already
  exists"), and every device refuses one in a log it verifies (`LogError::Recreated`). So
  neither a buggy client nor a relay colluding with a member can reset a Space's privacy
  under the devices.
- **History stays as it was.** What was said in the clear before the upgrade was readable by
  the relay and stays so. Upgrading doesn't rewrite the past. Pruning below checkpoints
  (later in M2) will drop it with the rest of the old ciphertext. A newcomer after the upgrade
  can still fetch that clear history, as in any `Closed` Space; that is stated, not hidden.

### Sealing at send time, not at write time
Defaulting to end-to-end made a write-time seal wrong. The first message of a new DM would be
sealed at epoch 0, before the creator's device had committed the other person in, and they
could never read it. So:
- A message in an end-to-end Space is signed and queued **in the clear on the device**
  (shown as *Sending*), and sealed by the network task when its group is **ready**. Ready
  means this device has the group, has no commit in flight, and (for an owner or admin) isn't
  about to add someone, counting adds still on their way to the relay. People with no key
  packages don't hold anything up (`stuck`).
- A message is always sealed at the group's **current epoch**. The sealed copy records its
  epoch, and a copy from an older epoch is sealed again before it goes out. This also closes
  ADR 0026's "outbox older than 4 epochs" gap. "Current" is the group's, not the device's:
  a device back from offline may still be behind a commit when it flushes, so the relay
  refuses a message sealed at an epoch the group has left (`STALE_SEAL`, from the clear
  framing) and the device seals again once it has applied the commit
  (`journey_m2::a_message_queued_offline_survives_many_commits`: five commits while Bruno is
  offline; everyone, including the five newcomers, reads what he queued).
  The relay dedupes by (author, client id), so a re-sealed message that already landed is not stored twice.
- A clear message refused with `SEAL_REQUIRED` (written just before the device learned of an
  upgrade) is not a failure. It waits until the device has applied `SpaceEncrypted` and has
  the group, then goes out sealed. Nothing is lost and nothing is sent in the clear twice.

## Journeys
- `journey_m1` now runs its DMs end-to-end unchanged (relaunches, offline writes, catch-up,
  photo backgrounds, typing). The late-joiner group test uses `--readable`, because it is
  about readable history.
- `journey_m2::a_readable_group_becomes_end_to_end_and_never_goes_back`: a `--readable` group
  with clear history, then Bruno queues a message offline, Ana runs `encrypt`, Bruno syncs.
  His clear message is refused, waits for his Welcome and lands sealed. Both have the same
  group, the relay holds the old clear text and none of the new, and a member's attempts to
  recreate the Space as `Closed` or upgrade it again are refused.
- `journey_m2::an_end_to_end_group_leaves_the_relay_only_ciphertext` adds a member with no
  MLS. She joins by invite, is refused in the clear with `SEAL_REQUIRED`, gets no commit, and
  doesn't hold up anyone's messages.

## Consequences
- The relay can no longer run server-side features over DMs and groups (search, previews,
  moderation). Those move to devices or to agents that are members.
- A message written before a device has its group (a newcomer waiting for their Welcome, an
  admin's device offline) waits as *Sending*. It doesn't fail.
- Admission checks who may write before what they wrote. A non-member gets "not a member"
  and learns nothing about the Space's privacy.

## At 1B users
- **Default traffic:** a new chat costs one key-package claim and three extra envelopes
  (commit, Welcome, checkpoint; ~3 KB). ADR 0022's model stores about 20B messages a day
  (600 TB hot at 1 KB over 30 days). Even at one new chat per daily user per day (500M, far
  above real rates), that is 1.5B extra envelopes, about 7%. At a few new chats per person a
  month, it is under 1%. Key-package claims are single-row Postgres statements on the same
  scale.
- **Upgrades:** one envelope plus the group's creation per Space. A mass migration of M1
  Spaces is bounded by owners opening the app, and spreads itself out.
- **Relay CPU:** the relay parses no MLS. `SpaceEncrypted` is one meta write in the append
  transaction it already runs.
- **Device CPU:** sealing at send time adds one SQLite read of the outbox per network tick,
  and only while something is queued.
