[Native surface index](../expo-native-surfaces.md).

## Shared implementation checkpoint

`apps/zoen/src/native-surfaces/snapshot.ts` checks snapshot item ID, app ID and deep link
against the actual Item, with bounded JSON/text/arrays and known local art. It produces
an immutable item/version/account/device projection. The default projection omits names,
values, rows, bars, artwork and domain actions. Explicit content selection exposes only
the compact content, never `view_json`, page bodies, account keys or database paths.

`catalog.ts` publishes at most 50 explicitly selected, currently accessible item IDs and
titles, with account/device/version metadata. The default selection is empty and
`hideInSpotlight` remains true. Callers must replace catalogs with an empty catalog on
sign-out or access loss. These navigation intents are not donation-enabled, so there
are no registered donations to remove. Calling the SDK's donation deletion API for
their names fails even on a fresh install.

`intents.ts` accepts navigation-only `zoen.openItem`, `zoen.openChat` and the app-owned
`zoen.openActivity` session link described above. It validates the invocation
age, exact parameter shape, current account/device, item version and current Space
access, checking the owner again after asynchronous reads. Unknown names and mutations
are rejected. This is repeatable navigation; the native queue may acknowledge only an
actual `opened` result. Invalid/retired actions need an explicit discard policy; transient
failures stay pending. App-owned Swift declarations now expose these read-only actions with local device authentication and opening dialogs. A single ready-session root consumer handles pending navigation; unknown actions and transient/foreign-owner failures remain pending, while known invalid/expired or stale-version requests are retired. Native metadata compilation and actual queue journeys remain pending.

`miniapp-widget.ios.tsx` is a real SDK58 pure layout registered as `ZoenMiniapp`, registered by the source-controlled CNG plugin. Signed v7 compiles the extension and its actual Portuguese redacted gallery preview
renders. Selected-content projection and refresh acceptance remain pending. It uses native Text timers, progress,
rows, rendering-environment adaptation and `privacySensitive` content. It has no local
mutation buttons. The text layout does not claim parity with animated baseline artwork
or the baseline's extra hike/ticket tiles.

Nine focused tests cover snapshot binding, unsafe content, bounded projection, default
redaction, explicit catalog selection, stale/foreign/unauthorized navigation and account
switches during asynchronous reads. They exercise shared code, not an installed extension.
