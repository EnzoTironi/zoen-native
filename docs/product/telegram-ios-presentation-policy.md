# Native context window policy

Telegram iOS `f1dd7a2dbd02cbbf513e75d5695d8d36d1cf5838` assigns `.Hide` only
to an extracted source with blur; a reference source assigns `.Ignore`
(`ContextControllerImpl:1941–1973`). Both lock the current interface orientation.
The status assignment is immediate (`Display/StatusBar:52–60`), not its separate
animated setter. At dismissal start the context restores `.Ignore`
(`ContextControllerImpl:1986–1995`); its presenting controller and orientation
lock survive until the actual animation completion (`2093–2107`).

Zoen now gives those traits to the existing native presentation owner:

- Ordinary attachment/gallery surfaces inherit status and orientation.
- Photos reference menus lock current orientation and inherit status, including
  an underlying extracted owner's hidden status. Placement, touch, backdrop and
  action animation are unchanged.
- Extracted File menus lock the current orientation and hide status. Dismissal
  immediately releases hidden status; actual removal releases orientation.
- Suspension, parent/window loss and retirement release effective ownership
  synchronously. Overlapping owners are weakly registered and release only their
  own policy. Resuming does not restore status ownership already ended by closing.

The root is created through the installed Expo 58 `ExpoReactDelegateHandler`
factory extension, registered in the source module configuration. It forwards
inherited status style, status visibility, home indicator and orientation to the
real native navigation/tab container, skipping the existing clear native overlay
controllers. It also handles the installed dev-client wrapper's additional
containment level. It does not reparent React views or install method swizzling.
The app-delegate subscriber uses Expo's actual orientation-intersection hook;
outside an admitted lock it preserves the generated phone/iPad orientation mask.
The lock reads the actual `UIWindowScene.effectiveGeometry.interfaceOrientation`,
never device acceleration or a guessed screen aspect ratio.

The CNG plugin sets `UIViewControllerBasedStatusBarAppearance=true`. In the same
change, the root iOS Expo StatusBar component is replaced by native-stack
`statusBarStyle` derived from the existing Zoen scheme. Android and Web retain
their previous component. The native factory and module are source-owned; no
generated AppDelegate, SceneDelegate or plist is edited by hand. No new package
or JS presentation-policy event is introduced. Theme changes and Apps' existing
landscape route options remain owned by native-stack. System previews retain
their own native controller traits.

Context policy admission requires the source-owned root to be the actual live
presenter. This is the current chat/File/Photos hierarchy. A menu requested while
an independent native controller is actually presented is rejected before
reparenting content; the owner does not pretend that the root controls that
controller's status bar. Ordinary overlay admission is unchanged.

Normal navigation, including chat, requests portrait through native-stack. Only the
fullscreen App route and its child App stack explicitly request the device-default
orientations, allowing portrait and landscape while that presentation owns the
screen. The generated phone/iPad supported masks stay broad enough for Apps; the
manifest does not impose a whole-app portrait lock. Native-stack restores the
underlying portrait mask after dismissal or route removal; a refused document
close retains the App presentation and its orientation. Existing context-menu
leases and the hardware-safe App close control are unchanged. Independent system
media viewers retain their native controller traits.

## SDK and CNG evidence

The installed versions used are Expo 58.0.16, ExpoModulesCore 58.0.23, autolinking
58.0.13, Expo Router 58.0.24, React Native 0.88.0 and react-native-screens 4.30.0.
Their actual factory, handler registration, subscriber intersection and native
window-trait implementations were read before using them. Official references:
[Expo SDK 58 StatusBar](https://docs.expo.dev/versions/v58.0.0/sdk/status-bar/),
[config-plugin development](https://docs.expo.dev/config-plugins/development-and-debugging/)
and [Expo Router Stack](https://docs.expo.dev/router/advanced/stack/).

Isolated proof lives in
`/Users/enzotironi/.codex/builds/zoen-presentation-policy-v254/`. Baseline is
immutable `9e38fb734efa53f8897bde335fcd34973ed6c57d`; the candidate applies this
batch's source changes. Both project copies reuse the installed node_modules;
neither installs dependencies. In each isolated app directory, the actual CNG
command is:

```sh
CI=1 EXPO_NO_TELEMETRY=1 EXPO_NO_GIT_STATUS=1 node node_modules/expo/bin/cli prebuild --platform ios --clean --no-install
```

Both completed. The sole Info.plist difference is the status-bar controller flag
false→true. AppDelegate and SceneDelegate are byte-identical; phone/iPad orientation
arrays are unchanged. Actual autolinking resolution and a package-scoped generated
provider include both native policy delegates. Main integration must run actual
CNG before Pods so the generated provider registers them; rebuilding Swift alone
does not apply this migration.

The actual UIKit/shared owner/menu/glass/Display/Lottie/compiled ExpoModulesCore
and React closure is SDK-typechecked with the authored policy and existing menu
tests. Tests use real UIKit controllers, the real factory handler and production
presentation owners; same-module copies only remove the target-module import.
The unchanged appearance/protocol and media-size declarations are extracted
verbatim from their existing owners for the bounded standalone check. No Expo,
renderer, policy or UIKit hierarchy implementation is substituted.

The five authored policy cases cover forwarding, exact current-orientation
mapping, generated phone/iPad mask fallback, separate status/rotation release,
overlap/reference inheritance, suspension, parent loss and invalid-root admission.
They are typechecked, **not executed** in this batch. TypeScript and focused
lint/format checks cover the accompanying app/config edits. No main app build,
Pods, core build, simulator/device/UI operation or service was used. Root runtime
qualification must cover day/night across all tabs, conversation/caption focus,
More and File menus, rotation while open and during return, system Preview/Cancel,
reopening and Apps landscape after policy release. Source/CNG/SDK proof is not
delivered status-bar or rotation proof.
