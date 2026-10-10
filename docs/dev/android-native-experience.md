# Native Android experience and upstream references

This follow-up replaces the eight-second raster mascot loop with native Android Canvas drawing. The Kotlin rig includes all eleven original poses, three head moods, continuous procedural time, drawing entrance and the original line geometry. The working head follows actual agent work. Animation pauses when the view is hidden, loses window focus, leaves the resumed lifecycle or the Android animator scale becomes zero. Static reduced-motion art is fully drawn.

The primary controls use the original 35 ink glyphs, with their signature motion beats, line wobble during interaction, selection draw-on and RTL mirroring for Back and Send. Android Material controls retain their touch targets, ripple, semantics and adaptive navigation. Files and Activity remain primary Android destinations alongside Chats and Store.

Haptic feedback uses `View.performHapticFeedback` with platform effects and API-level fallbacks. It respects system and view settings. Text and voice sends signal after signed storage; recording start, lock, cancellation, tile pickup, reorder, drop and confirmed unpin use their actual action states. Swipe gestures signal the threshold and return with a spring. Mini-app flips observe reduced motion live, including when closing.

Activity opens the native approval deck from the current iOS review, with a retained list view. Right and left decide one request; up and down create the shared engine's scoped, revocable standing rule. Sensitive requests disable standing approval. Buttons and accessibility actions offer the same decisions. A 4.5-second undo window precedes the signed write, survives activity recreation and cancels without a compensating event. Cards follow the finger, return with a spring, fly out after acceptance and slide back on undo. Confirmation feedback follows actual storage. Leaving Activity commits the pending choice; changing accounts or losing keys discards an unsigned choice.

## Versions checked

The remote comparison on 2026-10-10 used these exact commits:

| Role | Commit | Integration status |
|---|---|---|
| Shared backend/main | `e051f97f046ef8c98652cfe61edadd515f4b480d` | Ancestor of the Android branch; includes encrypted account and MLS recovery |
| Android starting point | `fdc492c0b8bfefa0d3acf119fc84d9767257b6ee` | Existing PR 41 recovery integration retained |
| iOS reading changes, PR 38 | `07a04de9434cd3e7d26ba30be00b5722360b9754` | Review behavior adapted to Android |
| iOS unified chat shell, PR 44 | `39ccb765957f9a6e2f95c48aebfbd1b8a3aac38c` | Review behavior adapted to Android |
| iOS reading in the roadmap review, PR 42 | `d9ebb3ed0208c52ce84600078e933213b2cdc905` | Changed Apple files match the reviewed PR 38 source |
| Pending backend approvals, PR 46 | `8e65c1bb17f08e66b658cffe77ae0b146703dfff` | Recorded separately; not merged into Android |
| Pending owner-device proof, PR 48 | `bb96f58a97f563598af790d831771f6ab0e2e112` | Recorded separately; not merged into Android |
| Pending model gateway, PR 49 | `693c2eb1b2d9422ade4e11931d17cb320eae34ea` | Recorded separately; not merged into Android |
| Durable key-package claims, PR 50 | `dd03f6e38a133aafc72644aa6b068ea6d1025715` | Merged into Android source; fresh combined JNI and native acceptance required |

Chats includes direct conversations, groups and communities with native kind filters and search. Incoming root messages keep the reading position and produce a counted new-message button. Own messages follow the bottom; replies, edits and system events do not count as new root messages. Initial unread placement uses incoming messages, including thread replies in the read count, and finds the first unread root. Pinned apps stay outside the scrolling history.

Run `python3 scripts/check-android-upstream.py --fetch` before publishing another update. It inventories all open PRs through `gh`, fetches their exact heads, verifies that the current main is integrated, compares original art hashes against both live iOS branches and checks changes in their Apple and mini-app source since review. It stops on a new interface change even when art hashes still match, including changes from a new PR. Updating the reviewed references requires reviewing the corresponding Android behavior. Backend candidates are compared against their merge base so an older branch does not appear to remove changes that landed later on main.

The expanded static comparison covers the upstream's 128 exported engine functions and 69 derived UniFFI records, enums and errors across the shared crate. Their signatures and type shapes match the pending backend. The Android branch also adds `member_roles`, `item_at`, `install_app`, `file_new_version_typed` and `MemberRoleDto`, and uses `CoreError.Storage.reason` for the JVM binding. These additions and the error-field adaptation must remain when upstream work is merged. All 132 Android engine functions match the generated Kotlin checksum inventory. Callback and session implementation changes need source review. This comparison does not prove the unmerged candidate's runtime behavior. Reports are saved under `build/` with UTC timestamps.

The integrated claim-replay core also requires the connected relay to advertise `key-package-claim-receipts` and a valid server clock. Protocol 4 alone is insufficient. Migration `0024_key_package_claim_receipts.sql` and the matching relay must precede client rollout; additions stay queued with an upgrade error on an older relay. Fresh local native runs must use the rebuilt relay, separately from the public deployment.

## Verification

The normal shared-core build generates Kotlin bindings and both ARM64 and x86_64 JNI libraries from the integrated backend. All 88 JVM cases and Android lint pass for the approval implementation. Independent Swift executables run the original equations to export test vectors: 98 mascot frames through 63.25 seconds match at a tolerance of `1e-12`; all 35 glyphs match brush ribbons, caps, blots, washes, partial drawing and animated frames at `1e-11`. The source hashes are included in both fixtures. These tolerances compare geometry, not raster pixels across platform renderers.

Six Android 15 interface cases pass for incoming-message reading, mixed unread placement, kind filters and search after restoration, historical viewport restoration, actual stored-send haptic dispatch and continuous mascot lifecycle/reduced-motion behavior. A separate cold-process TLS case passes. Full native acceptance and fresh CI results are recorded with source-specific evidence in PR 41. Earlier failed run logs remain preserved.

The 31f2ce0 APK passed all 61 native cases on Android 15 at both 720×1600 and 320×640. Its CI exposed two additional recovery issues: Android 9 cannot write the process temporary directory, and a group checkpoint on a recovered device does not yet mean its peer has admitted it. Backup and restore now keep plaintext scratch databases beside the application database in a private directory and remove database sidecars on success or error. Recovery acceptance waits for matching epochs and checkpoint digests on both peers within the existing 30-second deadline before testing delivery. All 69 shared-core unit cases and strict crate Clippy pass, including the cleanup and admission regressions. Rebuilt JNI acceptance is tracked separately from interim approval UI runs that retained the previous JNI libraries.

The deleted-backup case retains failed restore, no account installation, old-device revocation and deletion assertions. The Android 9 TLS fixture negotiates a shared TLS 1.2 cipher and preserves certificate rejection through Conscrypt's wrapped cause. The native suite now has 64 cases; full results for each published source and APK pair belong in PR 41. The shared Rust CI also exposed an unresolved CLI key-package refill journey, recorded with its original failure log.

At 2026-10-10 14:37 UTC the public relay still answered protocol 2 while the shared core requires 4. The anonymous probe sent no Auth or Register. The [public rollout](android-relay-release.md), tactile assessment on a physical phone and physical model execution remain external validation conditions. Emulator feedback receipts prove platform requests and stored behavior; they do not prove how vibration feels on hardware.
