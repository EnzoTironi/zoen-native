# Expo SDK 58 patch alignment — 21 October 2026

The installed SDK 58 packages lagged the current supported patch set, causing
Expo Doctor to reject an otherwise passing client CI run. This change aligns
the following direct dependencies through `expo install`, preserving SDK 58,
React 19.3.0, React Native 0.88.0 and the application's existing configuration.

| Package              | Resolved before | Resolved after |
| -------------------- | --------------- | -------------- |
| `@expo/ui`           | 58.0.21         | 58.0.22        |
| `expo`               | 58.0.16         | 58.0.17        |
| `expo-app-intents`   | 0.5.10          | 0.5.11         |
| `expo-asset`         | 58.0.17         | 58.0.18        |
| `expo-audio`         | 58.0.9          | 58.0.10        |
| `expo-camera`        | 58.0.11         | 58.0.12        |
| `expo-clipboard`     | 58.0.3          | 58.0.4         |
| `expo-constants`     | 58.0.12         | 58.0.13        |
| `expo-linking`       | 58.0.13         | 58.0.14        |
| `expo-notifications` | 58.1.5          | 58.1.6         |
| `expo-router`        | 58.0.24         | 58.0.25        |
| `expo-sharing`       | 58.0.19         | 58.0.20        |
| `expo-splash-screen` | 58.0.10         | 58.0.11        |
| `expo-status-bar`    | 58.0.3          | 58.0.4         |
| `expo-widgets`       | 58.0.21         | 58.0.22        |

## Upstream compatibility review

The [SDK 58 reference](https://docs.expo.dev/versions/v58.0.0/),
[upgrade guide](https://docs.expo.dev/workflow/upgrading-expo-sdk-walkthrough/)
and [release notes](https://expo.dev/changelog/sdk-58) establish the SDK baseline.
The individual package changelogs are captured from official Expo revision
`c121bf4589add7a2f3e17f981a4850db414c8534`, with public npm package integrity
metadata and source hashes in the evidence directory.

The reviewed patches include iOS hosted-view remount fixes, navigation transition
and safe-area fixes, a multi-lens camera default, optional clipboard/lock-screen
audio/status-bar APIs, and dependency updates. No new API is adopted here.
`expo` also changes the Swift Package Manager JSI build path; that release note
does not prove the app's CocoaPods build or runtime behavior.

The lockfile records 47 version additions/removals/changes, including transitive
SDK tools and platform binaries. Notable changes are `expo-modules-core` 58.0.24,
`expo-modules-jsi` 58.0.17, macros 0.22.0, `@expo/config-plugins` 58.0.10 and
`xcode` 4.0.0. `@expo/metro-config` explicitly requires `noxcturnal` ^0.1.8;
its platform package updates are part of that SDK dependency graph.

The resolved `@expo/inline-modules` 0.2.7 changes its config-plugin dependency;
the published updater bytes are identical to 0.2.6. The exact patch is renamed
to 0.2.7, and the existing version assertions follow that pin. Original/patched
source hashes remain unchanged. The real updater/parser/migration tests still
reproduce the unpatched duplicate and verify UUID reuse, memberships, atomic
refusals and isolated patch application/reapplication with `xcode` 4.0.0.

## Qualification and retained diagnostics

Dependencies are installed in this worktree's private, ordinary `node_modules`
directory. Its prior shared symlink is preserved under ignored `.expo/`; neither
the shared dependency tree nor the parent's installed application was changed.

The first CLI invocation could not detect the SDK before private dependencies
existed. A lock-preserving private bootstrap supplied them. The subsequent
`expo install` installed all requested versions and applied patches, then refused
automatic edits to dynamic `app.config.ts`. Its plugin scanner does not recognize
the existing configured App Intents/widgets applied by `with-native-surfaces`;
the suggested default status-bar plugin has no props and is a no-op. No duplicate
default plugins or validation exclusions were added. These failed commands are
retained as installation diagnostics, not passing qualification results.

After repinning the CNG patch, one fresh `npm ci` applied all three patches
without a version-mismatch warning. Online `expo install --check --json` reported
`upToDate: true`; Expo Doctor passed **20/20** checks. The existing focused CNG
test file passed **8/8** tests. TypeScript, Oxlint, Oxfmt and diff whitespace
checks passed. No Doctor rule, network check or package exclusion was disabled.

Evidence is retained at
`/Users/enzotironi/.codex/builds/zoen-expo-58-patch-v261/`.
This slice does not run the full JavaScript test suite, actual prebuild, Pods,
native compilation or simulator/device checks. Those remain the parent Native17
batch after integrating the UI changes; static and dependency checks do not
establish runtime compatibility or visual fidelity.
