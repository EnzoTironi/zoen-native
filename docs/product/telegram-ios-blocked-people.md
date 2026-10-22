# Blocked people recovery

You includes a Privacy and safety section with a working Blocked people destination.
The virtualized list reads the existing Core `blocked_people` projection and loads
profiles only for rendered rows. Opening a known profile preserves the existing
profile actions. A missing/unreadable profile has an honest label and retry; no
technical identity key is displayed.

Unblock confirms the selected person, calls the existing `unblock_person` command
once, then rereads Core. A command error also triggers a read, because the local
change may have completed before its response failed. It never retries the
mutation automatically. Failed list reads leave prior rows disabled until a
successful refresh. The Core remains the only durable blocked list.

The controller and rendered callbacks bind account, certified-device selection,
raw configured relay, session epoch, focus/foreground lifetime and read revision.
Leaving the screen, backgrounding or replacing the owner retires old callbacks;
already admitted Core work can settle normally. Reopening/foregrounding reads
again. No new backend authority, reporting endpoint or permission store is added.

The behavior reference is Telegram iOS
[`BlockedPeersController.swift`](https://github.com/TelegramMessenger/Telegram-iOS/blob/f1dd7a2dbd02cbbf513e75d5695d8d36d1cf5838/submodules/SettingsUI/Sources/Privacy%20and%20Security/BlockedPeersController.swift),
at the same pinned revision as the [primary UI guide](telegram-ios-reference.md).
The existing Zoen shared theme, rows, avatar and confirmation components supply
presentation; no Telegram client/backend implementation is imported.

Focused controller and compiled UI tests cover the source contract. Integrated
app checks and native VoiceOver, Dynamic Type, focus/return and confirmation
qualification belong to the next coherent iOS batch. This slice does not complete
the broader profile/settings/community scope in issue181.
