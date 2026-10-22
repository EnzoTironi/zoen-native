[Native surface index](../expo-native-surfaces.md).

## Apple baseline

`apple/Shared/Widgets/WidgetSnapshot.swift` validates seven content templates: stat,
progress, countdown, list, caption, ticket and photo. `SnapshotCard.swift` renders them
inside the app. `ChatTiles.swift` adds local presentation tiles and pinned strips.
`AppModel.swift` stores Home ordering/unpinning and chat tile ordering in UserDefaults.
Conversation pin/archive/mute preferences separately use the Rust Personal Space.

There is no WidgetKit extension target in `apple/project.yml`, or an ActivityKit,
AppIntent or AppShortcuts declaration in the baseline Swift sources. Comments describe
future widget-extension reuse. The in-app pinned-card journeys therefore do not prove
a launcher widget, Live Activity or Siri integration.

The portable `miniapps::snapshot` supplies each built-in miniapp's canonical compact
content. Native `ItemDetail.app.snapshot_json` already exposes it. It must remain the
source of displayed values; an extension must not recreate pet, poll, schedule or
approval business rules.
