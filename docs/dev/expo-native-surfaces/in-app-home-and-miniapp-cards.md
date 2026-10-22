[Native surface index](../expo-native-surfaces.md).

## In-app Home and miniapp cards

The source now defines four native tabs: Home (`/`), Chats (`/chats`), Files and You.
Activity remains a root Stack screen at `/activity`; the Home menu always opens it,
and a review tile appears when canonical requests are Pending or Stale. Chats aligns
its heading and native glass search/compose capsule in one row, with the same
18-point top and 20-point horizontal content padding as Home. The whole heading and
miniapp rail live inside the automatic-inset list; the separate Stack header is
hidden. There is no expanded search field. Search opens the actual `/search` route,
and the cards retain separate spacing below the heading.

`HomeProvider` is mounted under the canonical client. It reads actual `items` with
miniapp content and filters them against currently known conversation membership.
Personal or unknown cached spaces remain hidden until the core supplies an explicit
authorization projection for them.
Home renders explicit favorites followed by remaining miniapps; Chats renders the
accessible miniapps. An individual conversation rail filters by its own Space and the
independent hidden/order preferences described above.
Compact cards render the canonical snapshot using bundled Zoen artwork and open
`/miniapp/[id]`. Invalid snapshots have a bounded title/headline fallback. These
in-app cards do not require external widget-content consent and do not donate titles,
start Live Activities or publish anything to the system launcher.

Photographic cards preserve a clear image hero and place every caption/value in a
lower68–82% black gradient region. A contrast regression checks primary/secondary
inks against arbitrary image pixels, including pure white. Photo press feedback
uses scale rather than alpha to preserve text contrast. Installed visual acceptance
on actual photographic cards remains pending.

Favorites persist only Item IDs in a versioned device-local preference namespace
bound to identity and device. There are no implicit default pins. Restore/read/write
results are fenced across account changes; failed pin writes retain the prior pins,
and device erasure awaits their cleanup. Demo pins are volatile. Home's native organizer
and card-menu movement persist an explicit favorite order, accepted with isolated drag
and cold retention in v40. Independent per-conversation choices are implemented above,
with v50 isolated drag, hide-all/restore and cold-conversation retention checks.
Conversation pin/archive/mute commands retain their own meaning.

Home uses native SwiftUI add and per-card pin menus. Add opens the existing miniapp
creation flow, new conversation, Activity or search. On iOS 26+, the native tab bottom
accessory provides a stateless Search Zoen launcher while Home is selected. It stays
mounted, and the SDK58 documented `unstable_nativeProps.ios.bottomAccessoryHidden`
host option hides the entire accessory on other routes. Merely conditionally removing
its children left an empty rounded native plate on Chats in installed v12. The
installed React Native Screens host maps this flag to UIKit `setBottomAccessory:nil`,
rather than hiding only the React content. Actual signed v13/v15 screenshots confirm
the empty plate absent on Chats and Files. Search opens the search screen. Loading, failure and empty
states expose real state rather than synthetic content. Controller tests cover owner
changes, persistence failures, stale reads, access loss and cleanup.
Actual v12 screenshots show canonical Recipe/Countdown/Pet/Poll cards in Home and
Chats, and Recipe opened its native detail. Signed v15 also confirms the Files
heading and actions below the notch. The later v40 checkpoint above accepts isolated
Home menu movement, detail pinning, drag ordering and cold retention. Signed v50 also
accepts isolated Home and conversation drag, hide-all/restore and cold retention,
plus preserving delivery and menu/organizer inspection on the user's simulator.
