# UX audit: animations, haptics and micro-interactions

> Audit of the SwiftUI app (`apple/Shared`, `apple/iOS`, `apple/Mac`) on `main` @ `98f544e` (9 Oct 2026).
> Read-only: no code was changed. Each item lists **where** (file:line, paths relative to `apple/`), **what feels wrong**, and **the fix**.
> Bottom sheets are already being reworked by the UI executor. They are only **noted** here (see "Sheets, for the executor").
> House rules checked: no technical jargon in the UI; Back pops exactly one screen; Slack-style swipe approval cards; drag-to-reply; jiggle edit; Things-style confirmations; profile sheet on every name or avatar tap; hand-drawn animated art; Reduce Motion respected.
> Related: `docs/ux-motion.md` (haptic table and WOW moments). That doc is a policy; the code has **no central Motion token set**, so it drifts (see P2-1).

## Numbers behind the audit

| Signal | Count |
|---|---|
| Distinct `.spring(...)` configs in the app | **80** (13× `.spring(duration: 0.4)`, then a long tail) |
| `.easeInOut` / `.easeIn` / `.linear` animation sites | 9 (onboarding hand-off, globe, pet, voice pulse, camera flash, approvals slow throw) |
| Files that animate but never read Reduce Motion | **25** (incl. `OnboardingFlow`, `ConversationsView`, `CommunitiesView`, `VoiceEditor`, `PageScreen`, `InlineCamera`, `WabiChat`, both app roots) |
| Haptic systems in use | 2: the `Haptics` enum (≈190 calls) and raw `.sensoryFeedback` (9 sites) |
| `Haptics.error` | **does not exist**; failures use `warning` or nothing |
| `.buttonStyle(.plain)` with no pressed state | ≈ 60 sites (UniversalSearch 10, PetViews 6, StoreView 5, Onboarding 5, Conversations 4 …) |
| `.font(.system(size:))` (does not scale with Dynamic Type) | **122** (StoreView 21, SnapshotCard 16, ApprovalsStack 11 …) |
| Hover effects on Mac | 1 (`RadialMenu`) |
| VoiceOver announcements (`AccessibilityNotification`) | **0** |
| Skeleton / placeholder loading states | **0** (bare `ProgressView` in 6 places) |
| Message long-press menu (`contextMenu` on a bubble) | **none** |

---

## P0: fix first (breaks trust, the house rules or the core chat loop)

### P0-1 Engineering text shown to users (the "no jargon" rule)
- `Shared/Features/ItemView.swift:108` shows `item.origin`. The core builds it from the planner's `engineLabel` (`crates/roda-ffi/src/engine.rs:1264`, `:2762`), so a plan header can read **"Zoen · local planner (fallback: the on-device model failed — <raw Swift error>) · from "…""**. Mini-apps also show the `ui://` resource URI. The same string is shown in `Shared/Features/ActivityView.swift:275` (task rows).
- `Shared/Features/YouView.swift:87` shows `planner.availabilityLabel`, which can be "Local planner (fallback) · <reason>" (`Shared/Agent/AgentPlanner.swift:48-49`).
- `Shared/Features/YouView.swift:84`: a disabled button labelled **"Soon (weeks 5–6)"**.
- `Shared/Features/ActivityView.swift:250` "Approved · done (simulated)"; `Shared/App/AppModel.swift:410` "(Simulated: nothing was really paid.)".
- `Shared/Features/ItemView.swift:176` "Every change is a signed event in the Space’s log"; `Shared/Apps/McpAppViews.swift:131` "signed version in the core"; `Shared/Apps/Native/WabiChat.swift:302` "Sandboxed MCP View · no network"; `Shared/Features/ChatBackground.swift:564` "saved in the chat’s signed log"; `Shared/Features/AgentsView.swift:132,151,185,209` ("in the log", "Computed by the core", "a signed Grant", "from the signed log"); `Shared/Features/YouView.swift:280` "evaluated by the core", `:367` "append-only log … Ed25519", `:394` "hash …".
- "Arrives in a later build": `Shared/Features/ProfileSheet.swift:138,141`, `Shared/Features/SpaceView.swift:164`, `Shared/Apps/McpAppHost.swift:299,359`, `Shared/Apps/McpNative.swift:341`.
- **Fix:** split every `engineLabel` and `origin` into a user line ("Made by Zoen from “…”") and a hidden debug field. Rewrite the copy using the table in "Copy rewrites" below. Add a CI grep gate (`scripts/`) that fails on `log|core|signed|hash|MCP|build|simulated|fallback|planner` inside `String(localized:)` and `Text("…")`.

### P0-2 Raw errors in toasts and forms, with no error feel
- `Shared/App/AppModel.swift:295-298`: `perform()` shows `CoreError.message` / `localizedDescription` as is. That one function is the error path for most of the app.
- Also: `Shared/Sync/NewChatSheet.swift:165,182,195,240,252`, `Shared/Features/OnboardingFlow.swift:410` (handle taken, during sign-up), `Shared/Features/CommunitiesView.swift:275`, `Shared/Features/ProfileSheet.swift:314`, `Shared/Features/Pages/PageScreen.swift:191`, `Shared/Features/ParticipantsView.swift:181`, `Shared/Sync/AccountSection.swift:61,151`, `Shared/Sync/SyncModel.swift:69`.
- **Fix:** add `FriendlyError(_ error) -> (title, hint, retry?)` that maps each `CoreError` code to plain words ("That name is taken, try @enzo.t"). On failure, play `Haptics.error()` (new) and use a short horizontal shake on the field or row (opacity pulse under Reduce Motion). Offer **Try again** when the call can be retried.

### P0-3 The chat yanks you to the bottom while you read history
- `Shared/Features/SpaceView.swift:186-207`: every change to `entries.count` runs `withAnimation(.snappy) { proxy.scrollTo("bottom") }`, including other people's incoming messages, thread replies and system rows. `:222-224` does the same when an agent starts working. Scrolled up reading? You get thrown down.
- There is no "↓ new messages" pill and no unread divider.
- **Fix:** derive `isNearBottom` from the existing `offsetY` (`:126`) plus the content height. Auto-scroll only when near the bottom or when *I* sent the message. Otherwise show a glass "↓ 3 new" capsule above the composer: it scales in with a spring, plays `selectionTick` on tap, then scrolls with `.smooth(0.45)`. On open, land on the first unread with an "Unread" hairline divider.

### P0-4 Every model change re-animates the whole timeline (and the first load)
- `Shared/Features/SpaceView.swift:435-442`: `reload()` runs on every `model.revision` (any chat, typing, read marks) and wraps `entries = new` and `pinnedApps = apps` in `.spring(duration: 0.5, bounce: 0.2)`. On first open, every bubble plays its insertion (`:104` scale 0.92 from the bottom) while the list jumps to the bottom. Later, unrelated revisions animate the layout of every row, so bubbles wobble when someone in *another* chat types.
- **Fix:** diff by id. With no previous entries, assign without animation (`Transaction(disablesAnimations: true)`). Animate only when new ids were appended, and only those rows get the insertion transition. Skip the reload when this space's timeline hash didn't change.

### P0-5 Controls that pretend to work
- Camera shutter: `Shared/Features/InlineCamera.swift:97-101` plays a **success** haptic and a white flash, then `SpaceView.swift:164` says the photo won't be sent.
- Profile **Call / Video** (`Shared/Features/ProfileSheet.swift:137-142`) only show a toast.
- Header menu **Mute** (`Shared/Features/SpaceView.swift:423`) says "Muted on this device." but mutes nothing.
- Mac **Attach** is a permanently disabled + (`Shared/Features/SpaceView.swift:970-977`).
- **Fix:** hide each one until it's real (behind `Flags`), or wire it up. Never play a success haptic for something that didn't happen.

### P0-6 A failed message is a dead end
- `Shared/Features/SpaceView.swift:582-586`: `.failed` shows a red "Not delivered" with no action, no haptic, and no transition from "Sending…". Your only option is to retype it.
- **Fix:** make it a button, "Not sent · Tap to try again". Play `Haptics.error()` once when it fails and a 2-step shake on the bubble (none under Reduce Motion). Add a context menu with Try again / Delete. Cross-fade Sending → Sent → Failed with `.contentTransition(.opacity)` and post a VoiceOver announcement.

### P0-7 Long-press on a message does nothing useful
- `Shared/Features/SpaceView.swift:602-710` (`MessageRow`) has no `contextMenu`. Only `textSelection` is there, which opens the system text selector. That leaves no Copy, Reply, Reply in thread, React, Delete or Edit, all standard in WhatsApp, iMessage, Telegram and Slack.
- **Fix:** `.contextMenu(menuItems:preview:)` with the lifted bubble as the preview, plus a quick-reaction strip (6 hand-drawn reactions) above it. Play `Haptics.longPress()` (medium 0.7) when it lifts and `selectionTick` when moving across reactions. A reaction lands on the bubble with an ink "stamp" squash (fade under Reduce Motion). Keep drag-to-reply as is.

---

## P1: important polish

### P1-1 Composer: abrupt send/mic swap and haptics while typing
- `Shared/Features/SpaceView.swift:991-1007`: `micMode` comes from the text, but the only `.animation` modifiers are keyed to `recorder.phase` and `tooltip` (`:884-885`). The `.scale` transitions on send and mic therefore never run, so the swap is instant. The send button's dim/undim (`:1002`) is also unanimated.
- `:886` plays `selectionTick` every time the field goes empty → non-empty: a buzz on the first character of every message and when you delete the last one. That's overuse.
- **Fix:** `.animation(Motion.snappy, value: micMode)`, morph with `glassEffectID` between mic and send, and remove the tick (the visual morph is enough).
- `:1061` tooltip `Task` is never cancelled, so tapping twice makes it flicker. Store and cancel it.
- `:1044` slide-to-cancel at −110 pt has no "armed" feel. Add `selectionTick` around −80 pt, and tint the hint red when it's armed.
- `:959-961` opening the camera panel leaves the keyboard up, so camera and keyboard stack. Set `focused = false` first.

### P1-2 Missing haptics on the actions that matter
| Where | Action | Add |
|---|---|---|
| `Shared/App/AppModel.swift:328` | approve | `Haptics.success()` |
| `Shared/App/AppModel.swift:334` | deny | `Haptics.dismiss()` |
| `Shared/App/AppModel.swift:408` | approve all | `success` + numeric text on the count |
| `Shared/DesignSystem/Components.swift:180-183`, `Shared/Features/SpaceView.swift:770` | Approve / Decline buttons | via the model calls above |
| `Shared/DesignSystem/Components.swift:251,261` | Undo in a toast | `Haptics.undo()` (soft 0.6) |
| `Shared/Features/ActivityView.swift:198`, `Shared/Features/YouView.swift:204` | raise budget | `success` |
| `Shared/Sync/NewChatSheet.swift:95,172` | pick / unpick a person | `selectionTick` |
| `Shared/Sync/NewChatSheet.swift:178,191,248` | chat / group created, joined | `success` |
| `Shared/Features/OnboardingFlow.swift:285-294` | notify / location choices | `selectionTick` (the agents step already has one) |
| `Shared/Features/ConversationsView.swift:33-36`, `Shared/Features/CommunitiesView.swift:96` | pin / mark read | `selectionTick` |
| `Shared/App/AppModel.swift:304` | `show()` toast | haptic by kind: error → `error`, undo → none (the action already buzzed) |

### P1-3 Wrong or doubled haptics
- `Shared/Features/InlineCamera.swift:97`: the shutter uses `commit` (`UINotificationFeedbackGenerator.success`). A shutter should be a crisp rigid impact.
- `Shared/Features/OnboardingFlow.swift:375` plays `action()` and then `:389` / `:407` play `commit()` in the same tap, two haptics about 0 ms apart.
- `Shared/Features/Approvals/ApprovalsStack.swift:196` (commit) and `:198` (next card settles) fire back to back on every swipe, plus `:189` arrivalTick. This belongs to the UI executor's area: coalesce to one per gesture.
- `Shared/Navigation/Haptics.swift`: there is no `error`, and failures use `warning` (`PlusVoiceCapture.swift:62,117,122,128,135`, `SpaceView.swift:1079,1097`). A recording that is too short isn't a warning; use `error` for "didn't work" and `warning` for "are you sure".

### P1-4 Buttons and rows with no pressed state
- Rows: `Shared/Features/ConversationsView.swift:25-30` (the main chat list), `Shared/Features/CommunitiesView.swift:90-93`, `Shared/Features/ActivityView.swift:85,98,104`, `Shared/Features/FilesView.swift:64,74`, `Shared/Sync/NewChatSheet.swift:122-136`, `Shared/Features/ItemView.swift:155-162,171-184`.
- Controls: `Shared/Features/OnboardingFlow.swift:345-356` (**Continue**, the most-tapped button in onboarding), `:153` Back, `:173` Skip; `Shared/Features/ItemView.swift:240-246` (plan checkbox); `Shared/Features/MessageReplies.swift:168-175` (cancel reply); `Shared/Features/ConversationsView.swift:248-254` (your avatar).
- **Fix:** add a `RowPressStyle` (tinted highlight plus 0.985 scale, `Motion.press`) and use the existing `PressScaleStyle` / `IconPressStyle` everywhere. Lint `.buttonStyle(.plain)` in review.

### P1-5 Back doesn't return where you were (`go(.space)` teleports)
- `Shared/App/AppModel.swift:1188-1196`: `go(.space)` switches to Chats and **replaces** the Chats stack with `[.space(id)]`. Callers include Files "Open the chat" (`Shared/Features/FilesView.swift:219`), Store "Message" (`Shared/Features/StoreView.swift:838`), profile "chats in common" (`Shared/Features/ProfileSheet.swift:169`), search (`Shared/Features/UniversalSearch.swift:592-625`, `Shared/Features/SearchView.swift:48,98,150`) and Activity mentions (`Shared/Features/ActivityView.swift:98`). Back then lands on the chat list, not the Store listing or folder you came from, and any chat you had open in the Chats tab is silently closed.
- **Fix:** `push(.space)` on the current tab's stack (the tab bar hides inside a chat anyway). Reserve `go` for deep links and notifications.

### P1-6 Profile sheet missing on these avatars and names
- `Shared/Features/SpaceView.swift:485,489` (chat hero avatar and face pile), `:754` (request row), `:796` (typing row).
- `Shared/Features/ItemView.swift:107` (creator), `:311` (agent reaction), `:389` (versions).
- `Shared/Features/ActivityView.swift:157`; `Shared/Features/RequestReviewView.swift:20`; `Shared/Features/Pages/VersoesView.swift:41`.
- `Shared/Apps/Native/HikeViews.swift:90` (voters); `Shared/Apps/Native/PetViews.swift:427`, `Shared/Apps/Native/MapTap.swift:390` (leaderboards); `Shared/Apps/McpAppViews.swift:125`.
- `Shared/Features/Approvals/ApprovalsStack.swift:800,1032`; `Shared/Features/YouView.swift:390`; toasts `Shared/DesignSystem/Components.swift:258,269`.
- **Fix:** `.profileLink(persona)` on each. The FacePile should open the participants list.

### P1-7 Empty states missing
- New account with no chats: `Shared/Features/ConversationsView.swift:40` only handles "no search results". Show a hand-drawn Zo plus "Say hi to someone" with a big "New chat" button and invite.
- `Shared/Features/FilesView.swift:51-80`: with no files you see "Folders · 0 folders" over two empty white cards. Use `InkEmptyState(pose: .map, "Files you share in chats land here")`.
- `Shared/Sync/NewChatSheet.swift:111-143`: before you type, the sheet is a blank form. Show recent people and an "Invite a friend" row.
- Mac detail: `Mac/ZoenMacApp.swift:193` uses the stock `ContentUnavailableView("Pick a chat")`. Use the hand-drawn `InkEmptyState`.

### P1-8 No skeletons: loading is blank or a spinner
- The chat timeline is empty until `reload()` (`Shared/Features/SpaceView.swift:226`). Store, Spaces and Files lists, person search (`Shared/Sync/NewChatSheet.swift:119`), versions (`Shared/Features/Pages/VersoesView.swift:83`), the voice transcript (`Shared/Features/VoiceEditor.swift:496`) and the file save (`Shared/Features/Files/FileScreen.swift:28`) all use `ProgressView`.
- **Fix:** add an `InkSkeleton` component (paper-grey bubbles and rows with a slow ink "boil", static under Reduce Motion), using `.redacted(reason: .placeholder)` on real row views. Show it only after 150 ms so fast loads never flash.

### P1-9 Toasts: one-size, silent, not dismissible by swipe
- `Shared/DesignSystem/Components.swift:238-290`, `Shared/App/AppModel.swift:304-311`: there's no swipe-down to dismiss, only tap-anywhere (so a stray tap on the text kills an Undo). Errors and info have no haptic. Nothing is announced to VoiceOver. A capsule with `lineLimit(2-3)` clips at accessibility text sizes. Showing uses a spring but hiding uses `easeOut 0.3`; `iOS/ZoeniOSApp.swift:68` and `Mac/ZoenMacApp.swift:130` close with a default `withAnimation`.
- **Fix:** a drag-to-dismiss with velocity, `AccessibilityNotification.Announcement(text).post()`, a rounded rect instead of a capsule at AX sizes, `Motion.exit` for hide, and a pause of the timer while pressed.

### P1-10 Reduce Motion gaps
- `Shared/Features/OnboardingFlow.swift:103` (step slide), `:418` (plan bounce), `:468,551` (bouncy pills); `iOS/ZoeniOSApp.swift:59` (bar slides); `Shared/Features/ConversationsView.swift:192`; `Shared/Features/CommunitiesView.swift:46`; `Shared/Apps/Native/WabiChat.swift` (`FloatingHeart`); `Shared/Features/VoiceEditor.swift`; `Shared/Features/Pages/PageScreen.swift`.
- Loops: `Shared/Features/VoiceNotes.swift:492` (`repeatForever` pulse), `Shared/Features/SpaceView.swift:809-825` (`TypingDots` bob), `Shared/DesignSystem/Components.swift:341-355` (`ThinkingDots`).
- **Flashing:** `Shared/Features/InlineCamera.swift:100-101` is a full white flash, which should be dimmed or removed under Reduce Motion.
- `Shared/Features/SpaceView.swift:416` reads `UIAccessibility.isReduceMotionEnabled` once instead of the environment value, so it doesn't update live.
- **Fix:** route everything through `Motion` (P2-1), which picks the reduced variant automatically.

### P1-11 Dynamic Type
- 122 hard-coded `.font(.system(size:))` don't scale (StoreView 21, SnapshotCard 16, ApprovalsStack 11, PetViews 9, UniversalSearch 7, MessageReplies 4). The tab badge in `iOS/ZoeniOSApp.swift:343` is size 10.
- Fixed-height sheets clip large text: `Shared/Features/CommunitiesView.swift:125` `.height(280)`, `Shared/Features/ItemView.swift:81` `.height(320/390)`.
- **Fix:** use text styles, or `@ScaledMetric` for icon-adjacent sizes; size sheets with `.fitted` or measured detents (executor).

### P1-12 VoiceOver details
- Tap targets with no button trait: `Shared/Features/SpaceView.swift:553-554` (`SystemRow` opens the item), `:670` (tap a quote to jump), `Shared/DesignSystem/Components.swift:288` (toast).
- No announcements for "sent", "not delivered", an agent replying, approvals done or Undo, so a VoiceOver user gets no confirmation.
- `Shared/Features/SpaceView.swift:553` hard-codes "Você" (not localized).

### P1-13 Mac: no hover anywhere
- Only `Shared/Navigation/RadialMenu.swift` reacts to hover. Chat rows, Spaces rows, Store cards, toolbar glyphs and message bubbles (which should show reply/react on hover) give no hover feedback.
- **Fix:** add `.onHover` highlights through the shared `RowPressStyle`, plus `.pointerStyle(.link)` on links and a bubble hover toolbar.

### P1-14 New chat / group flow feels stock
- `Shared/Sync/NewChatSheet.swift`: it's a stock `Form` with a segmented picker. Results and picked chips appear without animation (`:94-137`). Success runs `dismiss()` and then a hard-coded 350 ms sleep before the push (`:256-262`), so you see the list flash before the chat.
- **Fix:** animate the `results`/`picked` ids with `Motion.snappy`, chips with scale+opacity transitions, and navigate on the sheet's `onDismiss` instead of a timer. A new group should get the `SpaceArtReveal` WOW moment like Create Space.

---

## P2: consistency and delight

### P2-1 No central Motion / Haptics tokens (80 spring variants)
Proposal (new `Shared/DesignSystem/Motion.swift`, mirrored in `docs/ux-motion.md`):

```swift
enum Motion {
    static let press      = Animation.spring(duration: 0.22, bounce: 0.30)  // button down/up
    static let snappy     = Animation.spring(duration: 0.30, bounce: 0.15)  // toggles, chips, swaps
    static let standard   = Animation.spring(duration: 0.40, bounce: 0.20)  // insert/remove, layout
    static let expressive = Animation.spring(duration: 0.55, bounce: 0.30)  // WOW, first-time moments
    static let sheet      = Animation.spring(duration: 0.45, bounce: 0.12)  // sheets, menus, panels
    static let exit       = Animation.spring(duration: 0.28, bounce: 0.00)  // dismiss, hide, fold
    static let scroll     = Animation.smooth(duration: 0.45)                // programmatic scroll
    static let reduced    = Animation.easeOut(duration: 0.20)               // Reduce Motion fallback
    /// Pick the reduced variant automatically.
    static func resolve(_ a: Animation, reduce: Bool) -> Animation { reduce ? reduced : a }
}
extension View { func motion<V: Equatable>(_ a: Animation, value: V) -> some View } // reads the env
func withMotion<R>(_ a: Animation, _ body: () throws -> R) rethrows -> R          // reads UIAccessibility
```

Haptics become semantic, with one entry point and a global rate limit (≥ 60 ms; a newer haptic replaces a pending one): `tap`, `select`, `send`, `success`, `warning`, `error` (new), `destructive`, `snap` (sheet/drawer detent), `longPress`, `lock`, `pickUp`, `drop`, `undo`. Add an in-app "Vibration" switch in You. Replace the 9 raw `.sensoryFeedback` sites with these, or wrap them so the switch and the rate limit apply.

### P2-2 Non-spring timing where a spring fits
- `iOS/ZoeniOSApp.swift:80` and `Mac/ZoenMacApp.swift:158` hand off from onboarding to the app with `easeInOut(0.4)`. WOW opportunity: Zo's head flies with matchedGeometry into the Zoen chat avatar.
- `Shared/Apps/Native/MapTap.swift:432` (`easeInOut 1.0` globe spin), `Shared/Apps/Native/PetViews.swift:148`, `Shared/Features/VoiceNotes.swift:492`.
- `Shared/Features/Approvals/ApprovalsStack.swift:502`: a slow throw exits with `easeIn(0.3)` instead of a velocity-seeded spring (executor).
- `Shared/Features/SpaceView.swift:88-91,214-218`: the search highlight uses 1.4 s in one place and 1.8 s plus different fade curves in the other. Unify them as a `Motion.highlight` token.

### P2-3 matchedGeometry / zoom opportunities
- Chat row avatar → chat top-bar avatar on push (`ConversationsView.swift:56` → `ChatTopBar.swift:74`), via `navigationTransition(.zoom)` or matched.
- Swiped bubble → the `ReplyComposerBar` quote (`SpaceView.swift:156-158`): the quote should fly down from the bubble.
- Mic → recording row (`SpaceView.swift:931-934`) with `glassEffectID` instead of a cross-fade.
- Spaces row → Space header (`CommunitiesView.swift:142` → `SpaceView.swift:488`).
- Store listing icon → detail hero (`StoreView.swift`).
- New-group chips → the new group's face pile.

### P2-4 Small abrupt changes
- `Shared/Features/ConversationsView.swift:84-86`: the unread dot's transition never runs because the change isn't animated. Mark as read (`:35`) has no `withAnimation`.
- `Shared/Features/ConversationsView.swift:339-351`: `ConnectionDot` appears and disappears without animation. `.help(c.error)` exposes the raw error. "Offline · 3 queued" → "Offline · 3 waiting to send".
- `Shared/Features/SpaceView.swift:337`: the pinned item bar pops in and out when the header menu toggles (no transition).
- `Shared/Features/OnboardingFlow.swift:158,177`: Back and Skip toggle opacity without animation.
- `iOS/ZoeniOSApp.swift:334`: re-tapping the current tab at its root does nothing. It should scroll to top, with `selectionTick`.
- `Shared/Features/MessageReplies.swift:88-97`: drag-to-reply ignores velocity, so a quick flick that ends before 64 pt doesn't count. Use `predictedEndTranslation`.
- `Shared/Features/SpaceView.swift:107`: `WorkingRow` is passed an "on device"/"local planner" label it never shows. Delete it so it can't leak later.

### P2-5 WOW moments worth adding (tasteful, once each)
- **First message from a new friend**: an ink "hello" doodle draws next to the bubble once per contact (`WowGate`).
- **First approval ever**: the stamp from Store install, "APROVADO", presses onto the card.
- **Group created**: reuse `SpaceArtReveal`.
- **Plan line completed**: a hand-drawn tick draws itself (`ItemView.swift:240-244` currently swaps SF Symbols).
- All of them play a fixed frame under Reduce Motion.

---

## Sheets, for the executor (not audited in depth)
- `Shared/Features/SpaceView.swift:180-183`: thread sheet is `.large` only, no drag indicator, no keyboard handling for its composer.
- `iOS/ZoeniOSApp.swift:94-115`: five root sheets plus one cover, each with its own detents. There's no shared `ZoenSheet` modifier for background, corner and snap haptic.
- `Shared/Features/CommunitiesView.swift:125` `.height(280)`, `Shared/Features/ItemView.swift:81` `.height(320/390)`: fixed heights.
- `Shared/Features/ItemView.swift:84`, `Shared/Features/ProfileSheet.swift:38,303`: `[.medium,.large]` with no snap haptic.
- `Shared/Sync/NewChatSheet.swift:256-262`: timer-based navigation after dismiss.
- `Shared/Features/ProfileSheet.swift:133,169`: `asyncAfter(0.15)` after dismiss. Use `onDismiss`.

## Copy rewrites (P0-1)
| Now | Suggested |
|---|---|
| "Zoen · local planner (fallback …) · from “x”" | "Made by Zoen from “x”" |
| "Soon (weeks 5–6)" | hide the row |
| "Approved · done (simulated)" | "Approved" |
| "(Simulated: nothing was really paid.)" | drop it; demo builds show one banner in You |
| "Every change is a signed event in the Space’s log" | "Every change is saved. You can always go back." |
| "Every tap is a signed version in the core…" | "Everyone in the Space sees the same thing." |
| "Sandboxed MCP View · no network · acts only in this mini-app" | "This app can only change itself." |
| "It’s saved in the chat’s signed log." | "Everyone in the chat sees this background." |
| "…it becomes a signed Grant in this Space’s log." | "Changes right away for this chat." |
| "from the signed log" / "evaluated by the core" / "Computed by the core" | remove the trailing label |
| "Voice calls arrive in a later build." | hide Call / Video until real |
| "Sending photos comes in a later build…" | hide the shutter until real |
| "Offline · 3 queued" | "Offline · 3 waiting to send" |

## Suggested order
1. P0-3 and P0-4 (chat scroll and re-animation): the most-felt problems, both in `SpaceView`.
2. P0-1 and P0-2 (copy and errors), plus the CI grep gate.
3. P2-1 tokens. After that, P1-1/2/3/4/10 become mostly mechanical replacements.
4. P0-5/6/7 (fake controls, failed send, long-press menu).
5. The rest of P1, then P2 delight.
