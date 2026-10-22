[Native surface index](../expo-native-surfaces.md).

## Current CNG integration

`app.config.ts` applies `plugins/with-native-surfaces.cjs`, composing the installed
SDK58 `expo-widgets` and `expo-app-intents` plugins. The extension uses bundle
`xyz.tironi.zoen.widgets` and isolated App Group `group.xyz.tironi.zoen.widgets`.
The primary SQLite database and Keychain service are not shared with the extension.
Push-to-start and frequent remote activity updates are disabled.

`NativeSurfacesProvider` publishes from the live canonical driver and refreshes on
store changes and foreground. Device preferences store only selected item IDs,
owner/device binding, explicit content choices and active system activity IDs.
`MiniappSurfaceControls` offers one device-wide chosen widget item, explicit content
reveal, opt-in Siri/Shortcuts titles, and start/stop for countdowns ending within eight
hours. Multiple widget instances currently use the same selected item; independent
per-instance selection is not implemented. A future redacted timeline entry is
scheduled at 15 minutes; WidgetKit owns scheduling, so this is not a guaranteed erasure
or refresh deadline. No private-content freshness guarantee is claimed while the
app is terminated. Live Activities use canonical ends and system stale dates.

The root mounts the provider beneath `ClientProvider`, the single
`NativeIntentConsumer` beneath ready navigation, and controls in the miniapp screen.
These mounts must be included before runtime acceptance. Surface deep links route
through `/surface`, reread item/access/version and then replace with the actual item.

Apply CNG and generate the Rust simulator framework:

```sh
cd apps/zoen
npx expo prebuild --platform ios
env -u SDKROOT DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer \
  node ../../scripts/build-expo-core.mjs ios --simulator
```

Build the generated workspace for the simulator UDID selected through Argent:

```sh
env -u SDKROOT DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer xcodebuild \
  -workspace ios/Zoen.xcworkspace -configuration Debug -scheme Zoen \
  -destination "id=$ZOEN_SIMULATOR_UDID" ARCHS=arm64 ONLY_ACTIVE_ARCH=YES \
  COMPILER_INDEX_STORE_ENABLE=NO
```

Keep Xcode's default ad-hoc simulator signing enabled. `CODE_SIGNING_ALLOWED=NO`
omits the simulated application entitlements and can cause Keychain access to fail.
The Rust simulator framework currently contains ARM64, so a generic destination
that also links Intel is not supported by this local artifact.

Install the resulting `Zoen.app` through `npx expo run:ios --binary <app-path>
--no-bundler`, which preserves the existing app data. Explicitly open the current
Metro dev-client URL afterwards if Expo selects a different server. Do not uninstall
the existing app or reset its account to make these features appear.

Native App Intents metadata extraction, extension entitlements,
App Group provisioning for device distribution, gallery rendering, Dynamic Island,
Siri warm/cold delivery and accessibility require runtime verification. No paid build
or developer portal changes were performed by this source integration.
