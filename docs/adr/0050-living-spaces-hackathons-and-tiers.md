# 0050: Living Spaces, hackathons and community tiers

Status: proposed. Default-off experiment; production rollout, cross-platform parity
and complete encrypted entitlement enforcement are separate verification gates.

## Problem

A community presented as a large conversation makes shared work, announcements,
events and membership decisions hard to find. Large encrypted communities also
need smaller conversation partitions without making people manage a hierarchy of
groups. Hackathons need a shared event lifecycle and private team workspaces.
Communities need named membership tiers without coupling v1 to billing.

## Decision

### One shared home, not a hierarchy

A community opens a Space home with a cover image, pinned announcements, live
pages, events, files, mini-app slots and conversations. Conversations are one
section, not the whole product. Underlying child conversations are separate MLS
groups (ADR 0026/0027); their hierarchy is implementation detail, never navigation
copy. Each child uses real membership changes and encrypted delivery, not a label
on a parent conversation. The experimental conversation allocator bounds each
partition to 256 members by default (administrator policy can choose 2–4096),
assigns admitted people internally, and exposes only their conversation
destination to the home. Ordinary conversation text is not posted to the parent
metadata log. Partition creation is not proof of production capacity.

Only the community owner and administrators may publish announcements. The core
checks the authenticated author on local writes and event replay; disabling a
button is insufficient. Pinned content and home configuration synchronize as
signed data under the parent's existing privacy policy. ADR 0027 explicitly keeps
`Community` parents `Closed` and relay-readable; this experiment does not silently
upgrade them or claim that the entire home is end-to-end encrypted. Root pages,
announcements and event/entitlement metadata retain that boundary. Conversation,
team and restricted-tier partitions are real end-to-end MLS groups. Membership
requests arrive in Notifications, where an
administrator approves or declines. Approval changes actual group membership and
must not report success before the underlying operation succeeds.

Live pages reuse the existing collaborative Markdown block/editor model (ADR
0040/0041). This experiment freezes agents: page content is visible and editable
by authorized people, but no planner, agent execution controls or agent cards are
part of the surface. Mini-app slots are labelled placeholders and never launch a
runtime, including the demo slot on a project submission.

The flag is off by default in both presentation and core entry points. Turning it
off must not delete user data or expose experimental write paths through legacy
screens. This is pre-launch code: no compatibility aliases or fallback protocol
shims. UniFFI bindings are regenerated, never hand-edited.

### Hackathon lifecycle

A Space may host a hackathon with an administrator-controlled schedule. The event
has registration, team discovery and formation, project submission, judging,
voting and results. Deadlines are persisted instants; countdowns derive from the
clock instead of decrementing an in-memory counter. Countdown text remains
legible with Reduce Motion and does not animate every second. A deadline is an
advisory schedule, not a trusted timestamp attestation: the relay does not attest
participant clocks. An authenticated administrator publishes `submissions_close`
to freeze registration, team formation and submissions and open judging/voting.
Replay uses this signed phase transition (`submissions_closed`), never a
participant's `at_ms`, for phase authorization. An elapsed countdown alone does
not open voting; administrators must close submissions explicitly.

Registration is attributed to the authenticated member. A person cannot register,
join a team, submit or vote on another person's behalf. Team membership is
explicit, bounded and validated against event registration. Creating a team creates
a separate encrypted conversation; joining a team must update that group's MLS
membership, not merely its displayed roster. Failures must not leave a usable
team claiming encryption or membership it does not have.

Submissions contain a project name, links, existing shared file references and an
inert demo mini-app slot. Accept only supported URL schemes, validate file access,
and keep submission edits scoped to the submitting team and allowed phase. Do not
fetch arbitrary project URLs as part of validation.

Judges are assigned by administrators; ordinary participants cannot elevate
themselves to judge. Judging scores and participant votes are distinct. Voting
must reject ineligible voters and duplicate ballots and apply a documented,
deterministic tie policy. Results are computed from accepted records, not from
client-supplied winner IDs. Finalization publishes the winners to announcements
without requiring an administrator to compose a second message. A stable event
result identity makes retries and replay idempotent; do not announce the same
result twice. An authorized client must be online to finalize in the client-owned
model: a local countdown is not a content-reading server scheduler. Unattended
server execution would require a separate privacy decision.

### Named tiers, without payments

Administrators define tier names such as Free, Member or Pro and assign members.
Tier names are presentation, not authority identifiers. Stable tier IDs drive
access to specific conversations, pages and events. A subtle badge communicates a
member's tier without replacing their identity or administrator role. Community
roles remain distinct: a tier cannot make someone an administrator.

Entitlements carry an explicit source:

- `Manual`: an authenticated administrator assignment.
- `InviteLink`: redemption of an administrator-issued, tier-bound invitation.
- `Paid`: reserved extension point; not an enabled source in v1.

The future paid source may supply verified entitlement evidence through the same
policy boundary. No purchase UI, checkout, provider integration, subscription
state or payment credential storage is built in v1. A client cannot obtain access
by sending `Paid`, changing a source tag, or asserting a tier ID.

A tier-bound invite must bind community, tier, issuer and redemption policy in a
scoped capability backed by an administrator-signed issuance record, with
expiration and revocation checked at redemption. The raw random capability is
returned only when issued; the synchronized record stores its digest, not the
secret. A copied tier identifier is not an invite. Removing or renaming a tier cannot turn
an old invite into a grant to an unrelated tier. Entitlement changes are signed,
synchronized and checked against the issuer's role at the time of the operation.
Tier changes use explicit deltas, not complete snapshot replacement: concurrent
administrators must not accidentally erase unrelated grants while those people
still hold keys. Invitation revocation is sticky; replaying an older issuance
cannot revive a revoked invitation. Local projections include accepted pending
outbox events, so immediate follow-up actions work before relay acknowledgement.

### Access is an encryption boundary

The core authorizes protected reads, writes, joins, registrations and downloads;
UI filtering alone is not protection. Restricted pages and events must live in an
appropriate encrypted partition, with keys delivered only to eligible members.
Encrypting restricted content to the entire parent community and hiding its row
would disclose it to every parent member and does **not** meet tier isolation.

Revocation removes future access and updates MLS membership/epochs where needed.
It cannot erase plaintext or keys already obtained by a formerly authorized
member. This limitation must be explicit, not described as retroactive deletion.
Attachments inherit their resource's access boundary. Never leak restricted page
text, event details or file metadata through the public home or notifications.

## Native design and platform contract

SwiftUI on iPhone and iPad is the only design source. iPhone uses native bottom
navigation including Spaces and Notifications; iPad uses a native sidebar/detail
split. Android mirrors iPhone; wide web mirrors iPad and narrow web mirrors
iPhone. Apple uses native Liquid Glass, `GlassEffectContainer` and glass button
styles. SF Symbol feedback reflects real actions and respects Reduce Motion;
Android uses matching animated vector feedback. Mini-app and agent functionality
remain frozen rather than represented by simulated success states.

## Verification and rollout gates

Required checks include default-off behavior, owner/admin/member announcement
permissions on append and replay, synced home metadata, actual MLS team creation
and admission, cross-device join approval, collaborative page convergence,
restart persistence, registration and phase guards, judge authorization, duplicate
voting, deterministic results and exactly-once winner announcements.

Tier tests must cover untrusted source tags, forged/expired/revoked invites,
nonadmin assignment, resource access and attachment leakage, demotion/rekey,
replay ordering and no escalation from tier to administrator. Passing a pure
policy test does not prove MLS key isolation or the built applications.

Capture the actual home, announcements, page and Notifications surfaces plus
hackathon registration/team/submission/results and tier management/access states
on iPhone, iPad, Android and narrow/wide web in light and dark. Synthetic fixture
captures must be labelled as fixtures and are not proof of multi-user behavior.
Evidence stays in `~/living-space-shots/`; third-party WhatsApp research captures
are private and must not be published. Record every unexercised requirement as a
gap in the local experiment report before enabling the flag for real users.

## At 1B users

Partitioning reduces the number of people holding each conversation's MLS state;
it is not a capacity measurement. Do not broadcast each team's content to the
whole community. Bound registration, roster, submission and ballot queries and
paginate large result sets. Countdown rendering stays local; no per-second
network polling. Result finalization requires deterministic conflict handling
across clients and durable idempotency. Entitlements need bounded indexed lookup,
not scans of the whole community on each message. No billion-user readiness claim
is established by this experiment or its local screenshots.

## Consequences

Community-wide end-to-end announcements and shared pages across all cohorts,
without a large parent MLS group, require a separate encrypted fanout/key-delivery
design. The current bounded conversations are independent cohorts, not a global
cross-partition transcript. Neither capability is established by this experiment.

The product gains a coherent shared home and event workflow while retaining
end-to-end encryption and a billing-independent access model. The cost is real
cross-platform core work: native and browser clients must share authorization,
collaboration and entitlement semantics. Until the verification gates are met,
this remains an off-by-default experiment, not a completed production feature.
