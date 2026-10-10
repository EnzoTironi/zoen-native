# Native Android experience and upstream references

This follow-up replaces the eight-second raster mascot loop with native Android Canvas drawing. The Kotlin rig includes all eleven original poses, three head moods, continuous procedural time, drawing entrance and the original line geometry. The working head follows actual agent work. Animation pauses when the view is hidden, loses window focus, leaves the resumed lifecycle or the Android animator scale becomes zero. Static reduced-motion art is fully drawn.

The primary controls use the original 35 ink glyphs, with their signature motion beats, line wobble during interaction, selection draw-on and RTL mirroring for Back and Send. Android Material controls retain their touch targets, ripple, semantics and adaptive navigation. Files and Activity remain primary Android destinations alongside Chats and Store.

Haptic feedback uses `View.performHapticFeedback` with platform effects and API-level fallbacks. It respects system and view settings. Text and voice sends signal after signed storage; recording start, lock, cancellation, tile pickup, reorder, drop and confirmed unpin use their actual action states. Swipe gestures signal the threshold and return with a spring. Mini-app flips observe reduced motion live, including when closing.

## Versions checked

The remote comparison on 2026-10-10 used these exact commits:

| Role | Commit | Integration status |
|---|---|---|
| Shared backend/main | `e051f97f046ef8c98652cfe61edadd515f4b480d` | Ancestor of the Android branch; includes encrypted account and MLS recovery |
| Android starting point | `fdc492c0b8bfefa0d3acf119fc84d9767257b6ee` | Existing PR 41 recovery integration retained |
| iOS reading changes, PR 38 | `07a04de9434cd3e7d26ba30be00b5722360b9754` | Review behavior adapted to Android |
| iOS unified chat shell, PR 44 | `39ccb765957f9a6e2f95c48aebfbd1b8a3aac38c` | Review behavior adapted to Android |
| Pending backend approvals, PR 46 | `54b9192c233cf65dbdf689fdfe952af89856f0b6` | Recorded separately; not merged into Android |

Chats includes direct conversations, groups and communities with native kind filters and search. Incoming root messages keep the reading position and produce a counted new-message button. Own messages follow the bottom; replies, edits and system events do not count as new root messages. Initial unread placement uses incoming messages, including thread replies in the read count, and finds the first unread root. Pinned apps stay outside the scrolling history.

Run `python3 scripts/check-android-upstream.py --fetch` before publishing another update. It records remote heads, verifies that the current main is integrated, compares original art hashes against both live iOS branches and compares the public facade signatures with the pending backend. All 60 existing facade method signatures match that candidate. Signature compatibility does not prove its unmerged runtime behavior. The report is saved under `build/` with a UTC timestamp.

## Verification

The normal shared-core build generates Kotlin bindings and both ARM64 and x86_64 JNI libraries from the current backend. All 81 JVM cases and Android lint pass. Independent Swift executables run the original equations to export test vectors: 98 mascot frames through 63.25 seconds match at a tolerance of `1e-12`; all 35 glyphs match brush ribbons, caps, blots, washes, partial drawing and animated frames at `1e-11`. The source hashes are included in both fixtures. These tolerances compare geometry, not raster pixels across platform renderers.

Six Android 15 interface cases pass for incoming-message reading, mixed unread placement, kind filters and search after restoration, historical viewport restoration, actual stored-send haptic dispatch and continuous mascot lifecycle/reduced-motion behavior. A separate cold-process TLS case passes. Full native acceptance and fresh CI results are recorded with source-specific evidence in PR 41. Earlier failed run logs remain preserved.

The pre-follow-up CI exposed a deleted-backup error expectation that predates the current relay's account-enumeration protection. The test still requires failed restore, no account installation, old-device revocation and backup deletion. The Android 9 TLS fixture explicitly negotiates a shared TLS 1.2 cipher and retains the certificate-rejection assertion, with its actual alert included on failure.

At 2026-10-10 14:37 UTC the public relay still answered protocol 2 while the shared core requires 4. The anonymous probe sent no Auth or Register. The [public rollout](android-relay-release.md), tactile assessment on a physical phone and physical model execution remain external validation conditions. Emulator feedback receipts prove platform requests and stored behavior; they do not prove how vibration feels on hardware.
