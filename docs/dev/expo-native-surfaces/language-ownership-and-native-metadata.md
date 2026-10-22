[Native surface index](../expo-native-surfaces.md).

## Language ownership and native metadata

The custom Expo entry resolves ordered device preferences once and initializes
the typed i18n-js catalogs before Router imports screens. Native bridge open and
browser-worker initialization require that canonical `en` or `pt-BR` tag. Rust
scopes the immutable engine language around synchronous calls and every async
poll, restoring ambient thread state. Language is not persisted in the account.
The current iOS/Web implementation reopens the process/page when preferences
change; coordinated Android foreground changes remain deferred.

Locale changes separators and grammar. Existing plan/grant cents retain their
documented BRL meaning; miniapp scheduling uses its explicit currency and scale,
and the signed owner policy remains USD/6. JavaScript formats/parses exact integer
strings with BigInt, preserving signed-i64 limits without floating-point rounding.
Rust still validates financial policy, including new-negative-cost refusal.

CNG configuration declares EN/PT support and generates translated permission
descriptions, Localizable strings and a separate AppShortcuts table. The plugin
links the native tables into the app/widget targets; widget worklets receive
bounded localized projection props and import no JS translation singleton.
App-owned native errors use English source keys with Portuguese translations.
Generated-resource inspection, Swift parsing, Foundation fixtures and CocoaPods
installation passed. Signed v6 accepts app-owned English and Portuguese main journeys; permission,
gallery and Shortcuts runtime acceptance remains pending.
