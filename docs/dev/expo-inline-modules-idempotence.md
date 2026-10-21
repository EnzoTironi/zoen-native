# SDK 58 watched-directory idempotence

Main CNG14 completed successfully but produced two synchronized groups for
`../app-intents`. The SDK's `@expo/inline-modules` 0.2.6 updater indexed the raw
`node-xcode` path token, while looking it up using `path.join('..', directory)`.
The actual CocoaPods `xcodeproj` 1.28.1 serializer writes `"../app-intents"` and
node-xcode retains those outer quotes. The lookup therefore missed the existing
group. The serializer also discards the unsupported `name` field, but that is not
the lookup key and is not the cause of this duplication.

The pinned MIT package is already resolved by the app lockfile. Its original
`build/xcodeProjectUpdates.js` SHA-256 is
`1f8b6b02c574d6f32b82c43a942bce0b1a183b59d94fc897dd11fc49684d4db8`;
the patched SHA-256 is
`b78d656073f53a8ff116cc5d8a71799a6d6372be7088713d56a99515ba2ad12f`.
The local patch uses Expo's existing `IOSConfig.XcodeUtils.unquote`, admits only
`SOURCE_ROOT` groups for this path lookup, and records a newly created UUID in the
map before processing another watched entry. Existing `patch-package` 8.0.1 and
the fail-on-error postinstall hook apply it; no new dependency is required.

The source-controlled native-surfaces Xcode mod repairs aliases already generated
for exactly the configured watched `SOURCE_ROOT` paths. The first existing main
group position determines the retained UUID. The repair unions exception
references, explicit folders and compatible explicit file types. It retains all
group and native-target memberships, remaps configuration anchors, and preserves
unrelated objects. Conflicting metadata or unknown references to a removed alias
abort before mutation. Generated projects are changed only by CNG.

The dependency patch is necessary even with that migration: installed CLI
`prebuildAsync.js` runs config mods, optionally installs Pods, then invokes the
inline-modules updater. A config mod alone cannot prevent the intermediate Pods
serializer from quoting the path again.

Use the lock-preserving SDK 58 command:

```sh
CI=1 EXPO_NO_TELEMETRY=1 EXPO_NO_GIT_STATUS=1 \
  node node_modules/expo/bin/cli prebuild --platform ios --no-clean --no-install
```

SDK 58 defaults to cleaning without `--no-clean`. Apply the existing dependency
patches before this command, then run the normal locked Pods installation. No
manual generated-project edits or dependency upgrades are part of this repair.

The focused test executes actual original and patched updaters, isolated
patch-package apply/reapply, node-xcode parsing, migration, membership preservation
and atomic refusal. It works with either an unpatched or already patched installed
package. Isolated CNG proof is recorded at
`/Users/enzotironi/.codex/builds/zoen-cng-watched-groups-v258/cng-proof.json`.
That proof starts from a copy of the actual CNG14 duplicate, preserves the original
UUID, and repeats real Ruby serialization, direct patched updating and CNG twice
per cycle. Locks and generated delegates remain unchanged. The original updater
reproduces the duplicate from the same one-group, quoted-path state.

The previous project's raw bytes were not retained; the negative fixture is a
semantic reproduction derived from the current project, not a reconstruction of
those missing bytes. Initial proof failures from an absolute patch-dir argument
and a missing shared locale fixture are retained separately. These are isolated
source/CNG/serializer results. Pods installation, native build, device interaction
and the complete app check remain the parent batch's responsibility.
