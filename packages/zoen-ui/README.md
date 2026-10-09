# Zoen UI

The root `@zoen/ui` export remains the mini-app kit. It uses the mini-app SDK and
the existing `@zoen/ui/tokens.css` stylesheet.

The application shell is a separate React 19 entry point. It imports no mini-app
SDK, reads no browser globals at module load, and can render on the server. Import
its stylesheet separately. All styles are scoped to `.zs-shell`.

```tsx
import { ZoenShell, ShellIcon, type ShellTheme } from '@zoen/ui/shell';
import '@zoen/ui/shell.css';

// The host owns these values and handlers.
<ZoenShell
  navigation={[
    { id: 'chats', label: 'Chats', icon: <ShellIcon name="chats" />, badge: 3 },
    { id: 'activity', label: 'Activity', icon: <ShellIcon name="activity" /> },
  ]}
  activeNavigationId={destination}
  onNavigate={setDestination}
  chats={visibleChats}
  activeChatId={selectedChatId}
  onSelectChat={openChat}
  searchQuery={query}
  onSearchChange={setQuery}
  theme={theme}
  onThemeChange={setTheme}
  title={currentTitle}
  onNewChat={createChat}
>
  {currentContent}
</ZoenShell>;
```

`ShellNavigationItem` and `ShellChatItem` accept a string ID type parameter, so
hosts can keep their own typed IDs. Search forwards each input change to the host.
The host supplies filtered chats, results, and content. The shell makes no network
requests and does not store chat data.

`theme` is explicitly `light`, `dark`, or `system`. System appearance follows CSS
`prefers-color-scheme`, including changes while the app is open. Theme persistence
belongs to the host. The local preview validates stored values and catches storage
errors, so blocked storage does not break the controls.

Shell colors follow the light and dark semantic values in
`apple/Shared/DesignSystem/Theme.swift` (`Palette`). The outer background uses
`background`, the sidebar uses `surface`, and the search field uses `surfaceMuted`.
Selected chats use `action` at 10% opacity; selected rail controls use 11% opacity
with the action color for their icon. The sample conversation uses the native
`myBubble`, `myBubbleText`, and `otherBubble` colors. Context overlay bands remain
transparent; individual pills and cards own their surfaces.

The rounded content frame fills the space beside the rail. Collapse lives in the
rail; New chat and a compact search field live in the secondary panel. There is
no full-width toolbar. Optional `title` and `headerActions` appear as a compact,
transparent overlay inside the content. It reserves no layout row, so messages
can scroll behind it. Hosts with their own content heading can omit `title` and
actions or choose their own initial content inset.

The rail is 64 px and the secondary panel occupies 280 px. Hiding the
panel returns its width to the content. Below 760 px, the rail stays 64 px and the
panel opens as a modal drawer. Escape, the close button, the backdrop, and selecting
a chat close it and restore focus to the opener. The background is inert while
the drawer is open. Drawer keyboard focus stays inside it. Reduced motion removes
layout, hover, and rolling theme motion.

Appearance uses one compact horizontal control. Dark, System, and Light roll in
cyclic order, with the selected mode centered and its neighbors visible at the
sides. Hovering a side highlights that target. Stable click areas are separate
from the decorative sliding icons. Selecting the center keeps the current mode.
Tab focuses the centered choice; arrows select the adjacent mode; Home selects
Dark and End selects Light. Focus remains at the center after selection. The
System monitor badge follows the effective light or dark scheme through CSS.

`headerActions`, `sidebarFilters`, `sidebarFooter`, and `emptySidebar` are host
content slots. `sidebarFilters` sits between search and the chat list; the host
owns the controls, their accessible labels, and the filtered chat data.
`sidebarTitle` and `searchPlaceholder` customize their labels. Set
`searchNavigationId` to the ID of the host's Search route. Selecting it opens the
panel or drawer and focuses the field after it becomes visible. Cmd/Ctrl-F does
the same while focus is in this shell. The shortcut preserves the current query
and selects it for replacement. It leaves browser Find alone when focus belongs
to another embedded view. Full-page hosts can set `globalSearchShortcut` to also
handle Cmd/Ctrl-F when the document body has focus; the preview enables this.
`searchInputRef` remains available for host integrations.
`defaultCollapsed` sets the initial desktop state. `className` supports host
layout overrides. The shell fills `100dvh` by default; an embedding host can
override `.zs-shell` height.

## Local preview

This is a working shell demo, not an authenticated web client. Its navigation,
search, appearance, chat selection, and new local drafts run in React. Sample
messages are static and explicitly labeled. No message delivery is connected.

Chats is one inbox for direct conversations, groups, and communities. The preview
offers optional All, Direct, Groups, and Communities filters that combine with
search. Every chat opens the same conversation surface. The Makers community
sample includes an expandable resource card, a sample guide, and member role
descriptions inside its conversation. There is no separate Spaces destination.
These local examples do not enroll members or enforce permissions. The generic
shell navigation API and existing `ShellIcon` names remain compatible; only the
preview's default navigation changes.

Activity opens as a stack of sample update cards. Previous and Next browse the
local examples, and Open chat opens the corresponding conversation. List is an
explicit view choice; its Cards button returns to the stack. Re-entering Activity
starts in Cards again. These sample updates are independent of any approval queue.

From the repository root:

```sh
npm ci --prefix packages/zoen-ui --ignore-scripts
npm run typecheck:shell --prefix packages/zoen-ui
npm run test:shell --prefix packages/zoen-ui
npm run build:preview --prefix packages/zoen-ui
npm run preview --prefix packages/zoen-ui
```

Open `http://127.0.0.1:4173`. The server binds to loopback only. Ctrl+C stops it.
Build output stays in ignored `preview/dist/`. The node tests import the actual
package subpath, verify that it excludes the mini-app SDK, and render it without
a browser. Visual and keyboard behavior must also be checked in the running
preview.

For visual review:

1. At desktop width, select all seven destinations: Chats, Activity, Store,
   Files, Your agents, Your context, and Search. Each has local demo
   content. Preview a catalog tool, open a sample file, choose an agent profile,
   and edit the local context draft. These actions use no account or backend.
   In Chats, open Marina, Saturday crew, and Makers community from the same
   inbox. Confirm the same conversation frame is used for each. Choose each
   kind filter, then All. Open Community resources in Makers community and
   expand its guide without leaving the conversation.
   In Activity, confirm Cards is the initial view, browse with Next and Previous,
   then select List and confirm Cards returns to the stack. Open a chat from each
   view and return to Activity; it should start in Cards again.
2. Select Search in the rail and search for `Marina`, open a result, then search
   for a name that does not exist. Both the secondary panel and main results
   reflect the query. Repeat with Cmd/Ctrl-F.
   Choose Communities and search for `makers`; the community remains in the
   results. Search for `Marina` with that filter, confirm the empty result, then
   choose All and confirm the direct chat is shown.
3. Hide the chat panel with the rail control and confirm that content reclaims
   the full 280 px. Select Search while collapsed and confirm the panel opens
   with the field focused. Hide it again and repeat with Cmd/Ctrl-F. Show the
   panel, then create a local draft with the New chat control.
4. Select the left and right theme targets. Confirm the clicked mode rolls to
   the center and a center click keeps it selected. Tab to the centered button
   and check arrows, Home, and End. Reload to check the preview preference. In
   System, change the operating system appearance and confirm the shell and
   monitor badge follow it.
5. At a width below 760 px, open the drawer. Tab and Shift+Tab remain in it. Close
   with Escape and confirm the opener receives focus. Repeat with the backdrop
   and a chat row. Open via Search and Cmd/Ctrl-F and confirm that the field, not
   another drawer button, receives focus. The conversation name remains visible
   inside the content when the drawer closes.
6. Enable reduced motion and repeat collapse and theme changes.
7. Scroll the sample conversation and confirm that messages continue behind the
   transparent context caption. The preview includes a longer static history for
   reviewing this behavior. It is not authenticated chat or live delivery.

Record desktop and narrow interactions and attach screenshots or video to the PR
with `gh --attach`, as required by this repository's review workflow.

## Drawer focus regression fixture

The build also produces `http://127.0.0.1:4173/focus.html`. This separate page is
outside the default preview navigation. It renders the actual exported shell with
a host footer containing a native `details`/`summary`, two roving buttons, and an
always-negative tab button at the end of the DOM order.

At a viewport below 760 px, open **Show chats**. Focus starts at **Close chats**.
Shift-Tab must reach **Roving stop A**, and Tab must wrap back to Close chats.
Tab forward and confirm **Host-slot disclosure** receives focus before Roving
stop A. The other rover and **Negative tab button (never in Tab order)** must be
skipped. With A focused, Right Arrow selects and focuses B; clicking B also works.
Tab from B must wrap to Close chats, and Shift-Tab must return to B. Escape must
close the drawer and restore focus to Show chats. The footer status identifies
the current last Tab stop.

SSR checks cover the fixture's real shell/slot wiring and both roving states.
Physical keyboard order, wrapping, and restoration need browser verification on
this page. Capture that behavior for PR review with `gh --attach`.
