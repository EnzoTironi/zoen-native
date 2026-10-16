# Living Space experiment

This experiment is intentionally **off by default**. A Space is a shared home,
not a visible hierarchy of groups. The internal encrypted conversation partitions
must not be presented as “groups of groups”.

## Product boundary

The canonical surface is native SwiftUI on iPhone and iPad. Android follows the
iPhone; narrow web follows iPhone and wide web follows iPad. The home contains a
cover, pinned announcements, collaborative pages, events, files, mini-app slots,
and a conversations section. Only owners and administrators may publish pinned
announcements; hiding a composer is not authorization. Join approval belongs in
Notifications. Existing collaborative Markdown editing is reused without agent
controls. Mini-app slots are inert placeholders, not runnable apps.

Native Apple glass is not approximated by a material fallback. Motion is feedback
for an actual action, not decoration on reading surfaces; Reduce Motion must be
respected. Public labels must not expose encryption implementation terminology.

## Local evidence

Screenshots stay in `~/living-space-shots/`; do not publish the supplied WhatsApp
research screenshots or reuse third-party names or phone numbers in fixtures.
Use synthetic fixture identities. Capture each of `home`, `announcements`, `page`,
`notifications`, `hackathon` (registration/schedule), `teams`, `submission`,
`results`, and `tiers`, in both light and dark, with these prefixes:

- `iphone`
- `ipad`
- `android`
- `web-narrow`
- `web-wide`

For example: `iphone-light-home.png`. Run:

```sh
python3 scripts/check-living-space-evidence.py
python3 scripts/test-living-space-evidence.py
```

The inventory proves only file presence and PNG dimensions. It does **not** prove
visual parity, admin authorization, live editing, or delivery of membership
requests between devices. Those require behavioral checks against the built
applications and explicit review of the captures. Missing captures must remain
reported as missing, never replaced with mockups labelled as running software.

## Delivery

Work belongs only on `feat/living-space`, created from `origin/main`. Do not push
or open a pull request. After local verification and commit, create the requested
bundle:

```sh
git bundle create ~/living-space.bundle origin/main..feat/living-space
git bundle verify ~/living-space.bundle
```

A range bundle requires the recipient to already have the excluded `origin/main`
history. Generated UniFFI source must come from the repository generator, never
manual edits. Build artifacts are not source deliverables.
