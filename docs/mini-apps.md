# Mini-apps in the chat (MCP Apps + Wabi's experiences)

Zoen creates live mini-apps inside a group. Each one's state is a **versioned Item in the Rust core**. Every tap becomes a signed version, everyone sees the same state and who did what, and the agent comments on what matters (a record, a new name, a closed poll).

Reviewed against main `e051f97` on 10 October 2026. Native widget pinning and Cards/List shell work are in [PR 44](https://github.com/EnzoTironi/zoen-native/pull/44), with [current native evidence](https://github.com/EnzoTironi/zoen-native/pull/44#issuecomment-6098239059). Built-in examples and remote MCP/catalog completion are separate; see [roadmap status](roadmap-status.md).

## What's there

| Mini-app | How it opens in the chat | UI | State in the core |
|---|---|---|---|
| **Group donkey** (Wabi "Group pet") | "what if we adopted a donkey for the group?" | Native SwiftUI | food/mood/rest (each action moves them, and they drift with time: hunger grows, rest recovers while asleep), nap, name, Donkey Dash leaderboard |
| **Donkey Dash** | the donkey's Play tab | SwiftUI `Canvas` at 60 fps | each person's best run + total runs |
| **MapTap** (Wabi "Games") | "Zoen, give us a geography game" | SwiftUI `Canvas` (orthographic globe with Natural Earth 1:110m) | today's 5 places, the same for everyone; guesses, km and points per person |
| **Dinner** (Wabi "Dinner plan") | "find a quick vegetarian dinner recipe for 3" | Native SwiftUI | servings, checked ingredients, which step someone is on |
| **Poll** | "poll: beach or waterfall on Saturday?" | **MCP View (HTML) in the sandboxed WKWebView** | votes; when it closes, the winner becomes a line in the Space's plan |
| **Group list** | "what to bring list" | **MCP View (HTML) in the sandboxed WKWebView** | items and who checked them; "Send on WhatsApp" asks for confirmation (simulated) |

The chat follows Wabi's pattern:
- The agent speaks without a bubble, next to an animated marker. Wabi uses a blue-purple orb; in Zoen the marker is **Zoen's face, the mascot's head** (hand-drawn and animated).
- Me in a black bubble; other people in #F2F2F2 gray.
- The agent's output is a **square live widget** plus a **card row**; tapping springs open a sheet (zoom).
- When another member acts, the widget changes for everyone, with a red dot and a heart floating up on the card.

The same mini-app shows up as a **live tile** on the Chats screen.

Illustrations follow Zoen's hand-drawn rule: the cards' doodles (pot, ballot, notepad), the donkey's unboxing reveal (a cardboard box that shakes, opens its flaps and lets him rise out) and MapTap's inked coastline are drawn at runtime with boiling ink lines; Reduce Motion freezes them. The donkey itself stays pixel art, animated (bounce, blink, eat, sleep).

The choice of mini-app comes from **Foundation Models on device** (`@Generable`, 12 s cap, told to answer in the app's language). Without the model, or past the timeout, keyword rules choose (both English and Portuguese), and the Item's origin says which one did.

## Localization

- Mini-app text comes from the core in the app's language: the default names (Donkey / Jumento), moods, activity notes, agent comments, MapTap places and hints, and the default recipe.
- MCP Views read `hostContext.locale` and switch their own strings (English by default).
- State written by people (names, options, items) stays as typed.

## MCP Apps (SEP-1865, version `2026-01-26`): what follows the spec and what's a stub

**Follows the spec:**
- **Resources and tools:**
  - `ui://roda/<app>` resources with `text/html;profile=mcp-app`.
  - Tools carry `_meta.ui.resourceUri` and `visibility` (`model` = the agent creates; `app` = the UI calls).
  - The host **refuses** `tools/call` from the UI for tools without `app`.
- **Handshake:** `ui/initialize` with `protocolVersion`, `hostInfo` and `hostCapabilities`.
  - The `hostContext` carries `theme`, `styles.variables`, `displayMode`, `availableDisplayModes`, `containerDimensions`, `locale`, `timeZone`, `platform`, `deviceCapabilities`, `safeAreaInsets` and `toolInfo`.
  - Then `ui/notifications/initialized` → `tool-input` → `tool-result` (a `CallToolResult` with `structuredContent`).
- **Messages:**
  - View → host: `tools/call`, `resources/read`, `ui/open-link` (with confirmation), `ui/message`, `ui/request-display-mode`, `ui/update-model-context`, `ping`, `notifications/message`, `ui/notifications/size-changed`.
  - Host → View: `host-context-changed` (theme) and `ui/resource-teardown`.
- **Sandbox:**
  - The spec's default CSP is injected (`default-src 'none'`, `connect-src 'none'`, `frame-src 'none'`…).
  - A content blocking rule covers http(s)/ws(s)/file.
  - Each card gets `WKWebsiteDataStore.nonPersistent()`.
  - No navigation, no new windows.
- **Grants:**
  - Every `tools/call` goes through the Grants evaluator (`roda-grants`).
  - The mini-app gets "Act" **only on its own Item**.
  - Irreversible or external tools return `needsConfirmation` and only run after the **native sheet** (the mini-app doesn't draw that button).

**Deviations and stubs (honest):**
- **No remote MCP transport.** The "MCP server" is local, inside the Rust core (`crates/roda-ffi/src/apps.rs`). There's no stdio/HTTP, no server `initialize`, no third-party server discovery.
- **Swapped transport.** Instead of an iframe's `window.parent.postMessage`, a document-start script replaces `window.parent` with a bridge to `webkit.messageHandlers`. The View can't tell the difference.
- **`tool-result` is re-sent on every new Item version** (live shared state). The spec sends one per execution.
- **Native apps.** The donkey, MapTap and Dinner are drawn in SwiftUI inside Zoen. Their tools, state and Grants are the same as the MCP server's, and the donkey also has an HTML View for other hosts. MapTap and Dinner have no MCP View (`has_view = false`).
- **Simulated.**
  - "Send on WhatsApp" in the built-in demonstration is a simulation. This statement does not describe real encrypted relay messaging or file/blob transport.
  - In the demos, Marina's, Lucas's and Ana's messages and actions are simulated as if they arrived through sync (`demo_member_*`, signed with their identity in the local log).
  - The donkey's "Emotes" only animate locally.

## Wabi's experiences (`/workspace/videos/wabi/site/SPEC.md`)

| # | Experience | Status |
|---|---|---|
| 4 | Group pet | **Done.**<br>• Pixel-art donkey; #C4D8E2 → #F2DFCD gradient.<br>• "Happy here" status; Food/Mood/Rest bars (gray track, blue fill).<br>• "Feed" and "Nap" pills; full-width "Donkey Dash"; an activity line; glass Care / Play / Leaderboard tabs; an "Emotes" pill.<br>• Renaming ("let's call him Peanut") updates the widget for everyone, and Zoen comments.<br>• Sleeping: dark background, Zzz, "Sleeping", Feed disabled, "Nap" becomes "Wake".<br>• Chrome-dino-style runner with a JUMP button, orange "+1" on carrots, pixel-font toasts and a group leaderboard. |
| 5 | Games (MapTap) | **Done, as a projected 2D globe.**<br>• Orthographic globe in `Canvas` (not SceneKit 3D); spins on its own in the widget and on the title screen (serif title, "Play").<br>• HUD "ENZO · YOUR TURN" with 5 dots and the score; glass card "ROUND 1 OF 5 / place / hint".<br>• Tap → pin (white ring, cyan center, ripple) → "Lock it in".<br>• The result card slides up with the km distance, a comparison with whoever already played and "Next place".<br>• Turn-based, same places for everyone. |
| 7 | Dinner plan | **Done.**<br>• Recipe with a servings stepper that rescales the ingredients live for everyone; square checkboxes.<br>• "Start cooking" → "STEP 1 OF 4", a large instruction, "Set 8 minute timer" and a sticky timer panel.<br>• No photo (an emoji on a gradient): nothing is downloaded. |
| 6 | Book club | Not done (the Dinner plan came first). |
| 1, 2, 3, 8 | Group trip, Bills, School, Live music | Later, as agreed. |
