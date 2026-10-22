[Native surface index](../expo-native-surfaces.md).

## Signed v50 conversation card preferences

The implementation adds separate presentation choices for each conversation. New
canonical miniapps appear in its rail by default; hiding a card removes only that rail
entry. It preserves the miniapp, its message card and history, and Home favorites.
The native card menu offers Hide in this conversation and Organize cards. Miniapp
detail offers Show/Hide and the same organizer; conversation details keep the organizer
reachable when every card is hidden. The native organizer separates visible and hidden
cards, with restore controls and drag handles for visible cards.

`ConversationCardsController` persists bounded, versioned Item-ID-only hidden/order
choices per Space in an identity/device/mode-scoped preference. Queued writes revalidate
the captured owner, exact current membership and canonical items; invalid permutations
are rejected, failed writes preserve the committed preference, and device erasure awaits
cleanup. Demo choices remain volatile. Home and conversation cards share the canonical
Home item source and membership projection rather than maintaining another content cache.
Restoring an owner waits for that owner's pending writes before reading its preferences.

Rendering uses subscribed immutable preferences, canonical items and membership. Both
native organizers give the default recycled, movable `List.ForEach` stable Item IDs and
resolve row content through the current canonical map. Successful saves do not remount
the movable rows. The conversation's movable `ForEach` is a direct child of `List`, with
its native section heading above the list; hidden cards use a separate static section.
Only failed saves change the native List key to restore its committed visual order. Busy
buttons and queued owner/scope guards serialize edits without transient row move metadata.
That checkpoint's full check passes 320 application tests, TypeScript, Oxlint and Oxfmt. Two
actual compiled-hook regressions fail against v42 and pass with the immutable projection.

Earlier signed candidates exposed distinct native failures: v41/v42 could retain stale
cards after a successful save, later hide/restore checks updated cards but lost drag
handles or move callbacks, and v48 Home saved a reordered preference while a success-key
remount blanked its rows. These failed candidates informed the final source design; they
do not establish drag or cold-relaunch acceptance.

Signed v50 source `4ef91160` removes the temporary QA traces and passes the final 320-test
check. On the isolated simulator, actual native Home swap-and-restore retained all three
rows; conversation swap-and-restore retained both rows and drag handles. Hiding two cards
down to none and restoring both updated immediately and retained native drag handles.
A cold restart retained Home's `[Recipe, Pet, Trip]` order. Final cold-conversation
checks retained Recipe left of Pet, while the hidden QA countdown remained visible as a
canonical message card in history. The conversation opened at 100% with an empty composer.

The user's simulator received v50 through a preserving install after an idle/empty-draft
preflight. Installed executable/JavaScript hashes and strict signature match the archive.
Normal restart retained four Home cards, Recipe's 6/8 progress and existing chat history.
Fresh chat entry retained its audio/receipt above the composer by approximately 36 points.
The actual native card menu exposed Hide in this conversation and Organize cards; the
organizer opened with four rows and drag handles, then Back returned to the latest history.
No hide, reorder, pin, content or audio-send mutations were performed on that simulator.

The 13-step primary-simulator diagnostic contains one failed short-label wait, followed
by a passing corrected menu-label wait, and four cross-tree warnings. It is not a clean
formal full-flow replay. Installed failure-injection rollback remains unverified. The v40
Live Activity evidence keeps its original attribution, and full product parity remains
unproven.
