# ADR 0027: Dynamic UI: native declarative views first, MCP Apps HTML in a sandbox second

Status: proposed (2026-10-08). Extends the mini-app host (`docs/mini-apps.md`, MCP Apps
`2026-01-26` already implemented in `crates/roda-ffi/src/apps.rs` and the sandboxed WKWebView).

## Context
Enzo wants MCP-Apps-style dynamic UI wherever Zoen is dynamic:
- live page blocks (ADR 0026);
- agent replies;
- Store apps;
- approval card details;
- notifications.

Primary sources, checked 2026-10-08:
- **MCP Apps** (SEP-1865, `modelcontextprotocol/ext-apps`):
  - Stable spec `2026-01-26`; draft updated through 2026-07-22; SDK v2.0.3 released 2026-09-25.
  - License: Apache-2.0 for new code and the spec, with older contributions under MIT during
    the relicensing.
  - The only defined content type is `text/html;profile=mcp-app`, rendered in a sandboxed
    iframe and spoken to over JSON-RPC `postMessage`. Other types are "reserved for future
    extensions", and `externalUrl` is deferred.
  - The draft adds:
    - app-registered tools (bidirectional `tools/call` and `tools/list`);
    - `sampling/createMessage` from the view, which the host should rate-limit, cost-control
      and approve;
    - `ui/download-file` and view-initiated teardown;
    - CSP `connectDomains`, `resourceDomains` and `frameDomains`, with "host MAY further
      restrict but MUST NOT allow undeclared domains";
    - camera and microphone permissions.
- **MCP-UI** (`MCP-UI-Org/mcp-ui`, Apache-2.0, client 7.1.1, 2026-05-09) is the community
  precursor that also had remote-DOM and external-URL types. Shopify `remote-dom` is MIT.
- **A2UI** (`a2ui-project/a2ui`, Apache-2.0, very active, pushed 2026-10-08):
  - Agents send a declarative JSON component tree (a flat list with id references, plus a
    data model), and the client renders it from its own trusted catalog of native components.
  - "Safe like data, but expressive like code."
  - Protocol v0.9.1 is the production release, and v1.0 is a release candidate.
  - Ships a Swift package (`A2UISwiftCore`, `A2UISwiftUI`, `BasicCatalog`; v1.0 support
    merged 2026-10-07).
  - Still labelled "early stage public preview".

## Decision
Three tiers, picked by the host in this order.

### 1. Zoen Views: declarative, native SwiftUI (default everywhere)
- **Format.** A2UI's message shape (surface, flat components with ids, data model, actions),
  with a **Zoen catalog**, carried as an MCP resource `ui://…` with mime type
  `application/vnd.zoen.view+json`. That's a Zoen extension in the space SEP-1865 reserves.
  We track A2UI v1.0 so third-party agents that already speak A2UI work. Before taking a
  dependency, we spike `A2UISwiftCore` (Apache-2.0) as the parser. The renderer is ours, on
  the design system: Liquid Glass, hand-drawn art, Dynamic Type, VoiceOver, haptics.
- **Catalog v1:**
  - layout: `Stack`, `Grid`, `Card`, `Divider`, `Spacer`;
  - content: `Text` (Markdown inline), `Image` (blob ref only), `Avatar`, `Badge`;
  - data: `Metric` (value, delta, unit), `Table`, `Chart` (Swift Charts: line, bar, area,
    pie), `List`, `TaskList`, `Timeline`, `MessageExcerpt`, `FileCard`, `Map` (MapKit, no
    network beyond Apple Maps);
  - input: `Button`, `Toggle`, `TextField`, `Picker`, `DatePicker`, `Slider`, `Stepper`;
  - feedback: `Progress` (no numbers by default), `Callout`, `Approval`.
  - Each component has fixed, on-brand styling. Agents choose structure and data, never
    colors, fonts or pixel sizes.
- **No code.** Only A2UI's declared formatting functions are allowed (dates, numbers,
  currency, pluralization in the app's locale). There are no scripts, URLs that load, or
  remote images. Limits: 64 KB per view, 500 nodes, depth 12, 10 updates per second per
  surface.
- **Actions** name a tool (MCP `visibility: ["app"]`) with typed arguments bound from the
  data model. Every action goes through `roda-grants` (Cedar):
  - allowed and reversible: it runs, with Desfazer;
  - `needsConfirmation`: the native approval card or sheet appears, drawn by Zoen, never by
    the view;
  - denied: a quiet shake and an explanation.
- **Updates.** A view is bound to an Item (live block, mini-app state, request), and a new
  Item version re-renders it with animated diffs: numbers roll, rows slide, and Reduce Motion
  turns this into a fade.
- **Where.**
  - live page blocks (ADR 0026);
  - agent replies (the agent returns a view inline instead of a wall of text);
  - approval card details (the tool declares a view for its proposal; tapping a card opens it);
  - Store app tiles and chat live tiles;
  - Activity rows;
  - notifications, widgets and Live Activities. These can only render the static subset,
    because a WebView isn't available there, which is another reason native comes first.

### 2. MCP Apps HTML in the sandbox (fallback, for full apps)
- Keep the existing host and add the draft features behind capability flags:
  - app-registered tools, callable by the Space's agents only under the app's grant;
  - `sampling/createMessage`, routed to the requesting member's model budget, and always
    approval-gated the first time per app;
  - `ui/download-file`, which saves into the Space as a File Item.
- Sandbox stays as built:
  - spec CSP with `default-src 'none'`;
  - a content blocking rule for http(s)/ws(s)/file;
  - non-persistent data store;
  - no navigation, popups or new windows.
- Declared `connectDomains` are **denied by default in encrypted Spaces**. Allowing them is a
  Cedar grant that the owner approves on a card saying in plain words "Este app quer falar com
  api.exemplo.com. O que você mostrar a ele pode sair do Zoen."
- Camera and microphone: the same pattern, plus the OS prompt.

### 3. First-party native mini-apps
The donkey, MapTap and Dinner stay hand-built SwiftUI, with the same tools, state Items and
grants.

### Host choice
- When a server offers both a Zoen View and HTML, Zoen renders the View.
- Other MCP hosts ignore our mime type and use the HTML.
- An agent reply with neither falls back to text.

### E2E
- View resources come from agent members, which send them inside the encrypted Space log, or
  from Store apps.
- A Store app's resources are a signed bundle (manifest plus resources), fetched as a blob,
  verified against the publisher signature and pinned by hash. A new version is a visible
  update the owner accepts.
- Rendering, data binding and action dispatch happen on the device. The relay never renders,
  proxies or sees a view or its data.

### Hooks
These are added to the ADR 0013 catalog:
- `ui.action.before`: device; can deny, rewrite arguments or ask; fail closed for safety hooks;
- `ui.action.after`;
- `ui.view.shown`: after; device only; for analytics with no content.

## Alternatives
- **HTML everywhere (MCP Apps only):** slow to start, not accessible by default, off-brand,
  impossible in widgets, notifications and Live Activities, and one script away from
  exfiltration if a domain is ever allowed. Kept for full apps only.
- **Our own schema from scratch:** an A2UI-shaped format costs nothing extra and lets
  third-party agents' UIs work.
- **Remote DOM (MIT):** still runs third-party JS, so it has the same sandbox burden as HTML.

## First slice
1. A Zoen View renderer with the 14 components the first live page needs:
   - `Stack`, `Card`, `Text`, `Metric`, `Table`, `Chart`, `TaskList`, `Button`, `Toggle`;
   - `Progress`, `Avatar`, `FileCard`, `Callout`, `Approval`.
2. Used in three places:
   - the ADR 0026 metric and task live blocks;
   - one agent reply ("Quanto gastamos esse mês?" answered with a `Metric` + `Chart` view);
   - the approval card detail for a page suggestion.
3. Actions go through Cedar. One action needs confirmation and shows the native card.
4. Journeys:
   - CLI: an agent member sends a view through the relay, and the second device's core
     renders it to a view model the CLI prints. The relay log holds only ciphertext. An action
     outside the grant is refused.
   - App: a person taps "Marcar como feita" in a live block, sees it checked on the second
     device, and swipes to approve the agent's edit.

## At 1B users
- Views are small: under 64 KB, typically 2–8 KB compressed, carried in events already
  counted in the log budget.
- Store bundles are immutable blobs, cached by the CDN forever.
- There's no server rendering tier at all.
