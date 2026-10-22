[Native surface index](../expo-native-surfaces.md).

## Verified SDK58 surfaces

`expo-widgets ~58.0.20` supports a pure `'widget'` layout using Expo UI SwiftUI views,
snapshot/timeline updates, configurable widget parameters, and Live Activity factories.
That layout runs in an isolated extension runtime with no hooks, asynchronous work or
access to the application engine. All required data enters through bounded props.
Its button callback can update local props while the app is closed; the application
interaction listener only runs while the app process is alive. Neither establishes a
signed Zoen mutation. [Expo Widgets SDK58](https://docs.expo.dev/versions/v58.0.0/sdk/widgets/)

`expo-app-intents ~0.5.9` is alpha. Real AppIntent, AppEntity, EntityQuery and shortcut
types must be declared in Swift compiled into the app target using Expo inline modules.
JavaScript cannot register those types or phrases dynamically. The invocation queue is
persistent and at least once; dispatch does not await a JavaScript result. On-screen
entity associations additionally require Xcode 27, and the relevant OS availability.
[Expo AppIntents SDK58](https://docs.expo.dev/versions/v58.0.0/sdk/app-intents/)

Live Activities fit ongoing events with a defined end, generally within eight hours.
Private content should remain in the application; Lock Screen information is visible
to nearby people. A distant trip countdown belongs in a widget rather than an activity
kept alive for days. [Apple ActivityKit](https://developer.apple.com/documentation/activitykit/displaying-live-data-with-live-activities),
[Apple Live Activities guidance](https://developer.apple.com/design/human-interface-guidelines/live-activities)
