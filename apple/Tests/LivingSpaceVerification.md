# Living Space Apple verification

The experiment is off unless `LivingSpacesEnabled` is set. The ordinary root and chat surface remain the default.

## Focused model checks

From `apple/`:

```sh
DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer swiftc \
  Shared/Features/LivingSpaceProjection.swift \
  Shared/Features/LivingHackathon.swift \
  Shared/Features/LivingTiers.swift \
  Tests/LivingSpaceProjectionJourney.swift -o /tmp/living-space-projection-test
/tmp/living-space-projection-test
```

Checks cover the home envelope, absent permissions, membership/hackathon data, capability-link routing and redacted private conversation identifiers. No capability values are printed.

## Simulator UI journeys

Generate the project with XcodeGen, then run only `ZoenUITests/LivingSpaceCaptureTests` using the `ZoenUITests` scheme. Use an ARM64 simulator: the local framework does not include an Intel simulator slice. Keep the framework stable for the entire test run, including `test-without-building`, because Xcode can re-resolve packages between cases.

The tests capture Home, announcements, the actual existing page editor, Notifications, hackathon overview, teams, project submission, results and memberships in both appearances. A separate experiment-off smoke case checks the existing chat shell. Full-screen iPad capture buffers may need a rotation-only transpose for landscape display; retain the raw originals and never crop or redraw them. On iPad they select landscape. Attachments include explicit point-coordinate accessibility frames; these are not pixel measurements of clipped images, corner radii or glass rendering.

### Fixture boundaries

- The demo fixture uses a dedicated `LivingSpaceFixture/demo-{appearance}.sqlite`, never the ordinary account database. Its signed join-request helper uses existing synthetic demo signers. The UI approves the actual projected request and verifies the member count. This is not proof of remote MLS transport.
- The account fixture uses a dedicated reset-on-launch `account-{appearance}.sqlite` and fresh synthetic identity. Its DEBUG-only vault holds new fixture keys in memory, never reads Keychain, writes key files or logs keys. Production Keychain behavior is unchanged.
- Both fixture modes route page-editor drafts through the same volatile fixture vault and a separate `LivingSpaceFixture/PageDrafts` directory. Normal page-editor Keychain storage and its draft directory are unchanged. The UI journey inserts a real edit, saves, leaves/reopens to verify persistence, restores the original text, saves and reopens again, and rejects draft-unlock errors.
- The account fixture does not start relay sync. Teams and membership partitions use actual local MLS creation and the outbox. Team messages, project submission and deadline-gated finalization are actual Core mutations. Remote delivery and another device receiving keys are not established by these screenshots.
- The single participant cannot vote for or judge its own team. Results can therefore legitimately contain zero votes and points. Scoring another participant is covered separately by Core tests, not this single-participant UI fixture.
- Invitations are rendered only as a sharing action and non-secret metadata (membership name, expiry and revocation status). The tests never open the share sheet or log the capability URL. They re-enter the membership screen before revoking, so revocation uses Core metadata rather than screen-local invitation state.

All fixture flags and capture deep links are DEBUG-only. The new feature uses native controls and glass. Its only explicit success transition is an opacity cue; Reduce Motion disables that animation. The countdown updates monospaced text without animated movement.
