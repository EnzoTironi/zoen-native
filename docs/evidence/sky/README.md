# Living sky evidence

Captured on this Mac with `/Applications/Xcode.app/Contents/Developer`, one iPhone 18 Pro simulator (iOS 27.0). The simulator was shut down after capture. These are **standalone previews of the production `LivingSky` component**, not screenshots of the full Zoen app or its marble glass. No glass or generated binding files were changed.

| Local hour | Screenshot |
| --- | --- |
| 06:00 dawn | [dawn.png](dawn.png) |
| 09:00 morning | [morning.png](morning.png) |
| 12:00 midday | [midday.png](midday.png) |
| 17:00 golden hour | [golden-hour.png](golden-hour.png) |
| 20:00 dusk | [dusk.png](dusk.png) |
| 00:00 night | [night.png](night.png) |
| 00:00 night, dark appearance | [night-dark.png](night-dark.png) |

[Contact sheet](contact-sheet.png) shows all seven, in table order.

## Design and overrides

Fixed local civil-time anchors, smoothstep interpolation including midnight. No location permission, network request, sunrise API, textures or display-link. Shared SwiftUI Canvas component on iOS/macOS; Compose Canvas on Android; shared web canvas CSS and clock. Existing custom chat backgrounds stay intact. The subdued wash protects primary/muted ink rather than forcing dark appearance at night. Clouds are hand-shaped cubic ink strokes; stars are slightly irregular crosses. Clouds drift at 1 Hz; Reduce Motion freezes drift and samples the clock once per minute. Apple removes the timeline when inactive; Android suspends updates outside RESUMED; web cancels its timer when hidden. Runtime lifecycle/power behavior has not been instrumented.

- Apple DEBUG launch argument: `-ZoenSkyHour 17` (fractional hours accepted); omit to follow clock. Also `LivingSky(previewHour: 17)`.
- Android DEBUG: `LivingSky(previewHour = 17.0)`, or string preference `sky_hour` in `zoen`. Release ignores override.
- Web loopback preview: `?skyHour=17`; hosted production ignores it.

## Validation

- PASS: `bash scripts/test-living-sky.sh`: production Swift model, all anchors and midnight continuity, invalid inputs, negative wrapping and 1,440 minute samples; web model continuity and primary/muted ink AA contrast in both schemes including conservative cloud/star overlap.
- PASS: `cd packages/zoen-ui && npm run typecheck:web && npm run test:web` (19 tests).
- PASS: standalone iOS component build via `xcodegen generate --spec docs/evidence/sky/project.yml`, then `xcodebuild -project docs/evidence/sky/SkyPreview.xcodeproj -scheme SkyPreview -sdk iphonesimulator -configuration Debug -derivedDataPath /tmp/zoen-sky-preview CODE_SIGNING_ALLOWED=NO build` with the developer directory above.
- BLOCKED: full ZoeniOS build: required `apple/Packages/RodaCore/RodaFFI.xcframework` binary is absent. Bindings were not regenerated or edited.
- BLOCKED: `cd android && ./gradlew :app:compileDebugKotlin :app:testDebugUnitTest`: no Java runtime installed. Android unit tests added but not run.
- NOT VERIFIED: full app/glass contrast, macOS build, Android rendering, browser rendering, live time-zone changes, Reduce Motion toggles and background/resume runtime tests. Numerical contrast tests cover the sky palette, not every existing UI material or text style.

Keep at least 5 GiB free before builds. Capture used about 3 GiB total of available space, leaving over 260 GiB free. No sudo, brew, force-push or merge was used; the personal `~/zoen/zoen-native` checkout was not touched.
