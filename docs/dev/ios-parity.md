# iOS-source visual parity

**iOS is the only design source.** Android and narrow web must reproduce the iPhone
layout; wide web must reproduce the native iPad layout. Match spacing, typography,
icons, glass, motion and haptics, rather than reinterpret the design. Inspect SwiftUI
first. Web glass must reproduce tint, edge highlights and backdrop filtering as well
as geometry. Static screenshots do not prove animation, interactions or haptic parity.

Source audit: the current `apple/iOS/ZoeniOSApp.swift` root uses a navigation stack per
tab, with no adaptive iPad sidebar/detail shell. The wide capture profile records that
actual implementation. It must not be described as proof of the requested sidebar,
detail split view, composer and toolbar parity until a canonical native iPad design
exists. Do not substitute the macOS shell or invent an iPad reference.

## Fresh current-source rendering

Use Python 3.12 and `Pillow==11.3.0`. Native prerequisites: Xcode/iOS 26, XcodeGen,
Rust Apple targets; Android SDK 36, NDK 27.1.12297006, Java 17, cargo-ndk and a fresh
API 35 emulator. Web uses Node 24, current `packages/zoen-ui` sources and Playwright
WebKit on macOS, retaining platform font/rasterization differences in the metric.

```sh
python3 -m venv .venv-parity
. .venv-parity/bin/activate
python3 -m pip install Pillow==11.3.0
python3 scripts/test_parity_compare.py

# macOS: boot a 402 x 874 point iPhone simulator.
DEVICE=$(xcrun simctl create NativeParityPhone com.apple.CoreSimulator.SimDeviceType.iPhone-17)
xcrun simctl boot "$DEVICE"
python3 scripts/parity_capture.py ios --device "$DEVICE" --viewport narrow --output parity/ios-narrow

# macOS: iPad Pro 13-inch M5, landscape (set by native capture test).
IPAD=$(xcrun simctl create NativeParityPad com.apple.CoreSimulator.SimDeviceType.iPad-Pro-13-inch-M5-16GB)
xcrun simctl boot "$IPAD"
python3 scripts/parity_capture.py ios --device "$IPAD" --viewport wide --output parity/ios-wide

# Android host, boot a fresh API 35 emulator first.
python3 scripts/parity_capture.py android --output parity/android

# Web host: build and start the current app using the Session preview service locally;
# the CI workflow starts its own server on its isolated runner.
npm ci --prefix packages/zoen-ui
npm run build:web --prefix packages/zoen-ui
npm install --prefix /tmp/parity-playwright playwright@1.56.1
/tmp/parity-playwright/node_modules/.bin/playwright install webkit
# Supply the current app's preview URL; its parity entry is /?parity=1.
node scripts/parity_web_capture.mjs --url '<preview URL with ?parity=1>' --viewport narrow --output parity/web-narrow --playwright /tmp/parity-playwright/node_modules/playwright/index.mjs
node scripts/parity_web_capture.mjs --url '<preview URL with ?parity=1>' --viewport wide --output parity/web-wide --playwright /tmp/parity-playwright/node_modules/playwright/index.mjs

# Bring all fresh artifact directories onto the comparison host.
python3 scripts/parity_compare.py --ios parity/ios-narrow --android parity/android --config scripts/parity_config.json --output parity/report/android-narrow
python3 scripts/parity_compare.py --ios parity/ios-narrow --web parity/web-narrow --config scripts/parity_config.json --output parity/report/web-narrow
python3 scripts/parity_compare.py --ios parity/ios-wide --web parity/web-wide --config scripts/parity_config_wide.json --output parity/report/web-wide
```

On a Mac whose selected developer directory is Command Line Tools, prefix native
commands with `DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer`.
Capture output directories must not exist. There are no checked-in PNG baselines or
fallbacks to old files. Use disposable devices: Android capture changes display
size/density, timezone, font scale and animation settings. CI uses separate current
native/macOS-WebKit render jobs, then compares those same-run artifacts.

## Fixture producer contract

All producers must actually apply `native-parity-v1` before seeding: seed `128`,
clock `2026-01-15T12:00:00Z`, locale `en_US`, timezone `UTC`, identical names,
messages, ordering and unread state. Freeze time-relative UI and settle animated
art at a deterministic phase. Merely changing a status clock or writing metadata is
not a deterministic fixture. Native hooks fail if the fixture cannot be applied.

Android receives instrumentation `parityFixture` and `parityRevision`. iOS receives
`ZOEN_PARITY_FIXTURE`, `ZOEN_PARITY_REVISION` and `ZOEN_PARITY_VIEWPORT` through the
`.xctestrun` environment; the UI test forwards fixture values to its app.

Each producer renders `chats`, `chat`, `composer`, `spaces`, `settings`, `onboarding`,
each in light and dark. Composer contains “See you tomorrow” and the actual platform
keyboard where present. Browser keyboard absence and native keyboard differences
count: do not mask them or draw a fake keyboard to pass the comparison.

Android writes `<screen>-<mode>.png` and `manifest.json` in
`cache/device-evidence/android-parity/`. iOS attaches screenshots named
`ios-<screen>-<mode>` and JSON named `parity-fixture.json`. Only light/dark suite
methods run, avoiding the separate onboarding-only test's duplicate frame.

Web exposes `window.__ZOEN_PARITY__ = { async render(config), manifest }` only in
its explicit parity entry. `render` applies the real app fixture and navigates to
`config.screen`/`config.appearance`, resolving when ready and at a stable phase.
It receives the fixture fields below plus `screen`, `appearance`, and
`viewport: "narrow" | "wide"`. Playwright sets a fixed Date before loading, awaits
this hook and fonts, then captures the real app. It injects no hiding/recoloring CSS.

Every producer emits these matching fields; the orchestrators never fabricate them:

```json
{
  "fixture_id": "native-parity-v1",
  "fixed_time": "2026-01-15T12:00:00Z",
  "locale": "en_US",
  "timezone": "UTC",
  "seed": 128,
  "source_revision": "the-current-git-commit"
}
```

## Normalization and honest metric

Only declared OS-bar crops and uniform full-frame resampling are allowed:

| Profile | Source geometry | OS-only crop (physical px) | Comparison / web CSS viewport |
|---|---|---|---|
| iPhone | 1206 × 2622 at 3× | top 186, bottom 102 | 402 × 778 |
| Android | 1206 × 2478 at 480 dpi | top 72, bottom 72 | 402 × 778 |
| iPad landscape | 2752 × 2064 at 2× | top 48, bottom 40 | 1376 × 988 |
| Web narrow / wide | CSS viewport above at 3× / 2× | none | same content size |

Phone geometry is measured on the existing 402 × 874 point iPhone simulator. The
wide iPad safe-area configuration is provisional until verified against actual
fresh iPad captures; a configured crop is not evidence of a measured safe area.
Verify all OS-bar measurements when device/runtime versions change. Never adjust
crops to hide app navigation, headers, composer, keyboard controls or glass.

Lanczos resampling preserves the full cropped frame. Aspect-ratio disagreement
above 0.002 is an error, not permission to squeeze controls. There are no per-screen
masks, shifts, fuzzy color tolerances, blur or content alignment. A pixel differs if
**any RGB channel differs**; font rasterization and anti-aliasing count.

The default maximum is **2.5%**. `--threshold` or the workflow dispatch input makes
changes explicit, but red means fix the replica, not silently increase tolerance.
Every screen is evaluated after a failure. Per platform and viewport, `report.json`
contains percentages/errors, fixture and crop metadata; `report.md` is the human
table; each comparable pair emits normalized frames and an RGB diff PNG. CI runs
all three profiles even if one fails, uploads reports/diffs and shows all tables.
Exit 0 means all pairs pass, 1 means a screen failed, 2 means invalid/missing fixture
or configuration. Missing captures are never treated as zero-difference frames.

## Verification limits

Unit tests prove comparator/extraction mechanics, not app fixture correctness or
visual parity. Only fresh native/web captures for all 36 comparison pairs and a
passing report prove this static check. Review raw frames and test motion/haptics
on devices separately. An absent canonical iPad split view remains a source/design
gap even when the current wide baseline is rendered successfully.
