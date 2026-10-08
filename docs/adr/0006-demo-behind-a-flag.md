# ADR 0006: The demo story only behind a dev flag

Status: accepted (milestone 1)

## Decision
A normal launch is real: no seeded people or chats. Onboarding asks for a name and @handle
and creates the account (works offline). `-RodaDemo YES` (or the screenshot flags
`-RodaStory` / `-RodaResetDemo`, which imply it) seeds the old story into a device that has no
account. Creating an account wipes demo data.

## Consequences
Screenshot scripts need `-RodaDemo YES`. The demo's agent replies and mini-apps keep using
the on-device planner until the agent runtime (milestone 3).

## At 1B users
No effect on the server. It keeps installs honest: a real install never ships seeded rows
that the relay would have to reject or migrate.
