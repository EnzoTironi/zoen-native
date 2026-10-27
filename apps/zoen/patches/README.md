# Dependency patches

`@expo+inline-modules+0.2.7.patch` fixes the SDK 58 watched-directory lookup after
CocoaPods' `xcodeproj` serializer quotes the same `SOURCE_ROOT` path. It uses the
installed Expo Xcode utility to compare the path's value and records newly created
groups immediately. It changes no dependency version. The existing native-surfaces
config plugin separately reconciles aliases generated before the patch, preserving
the original UUID, exceptions and memberships or refusing conflicting metadata.
See [the reproduction and CNG contract](../../../docs/dev/expo-inline-modules-idempotence.md).
Requalify the pinned patch when upgrading this package and remove it when the
released updater passes the same quoted-path and Ruby serialization regressions.

`react-native-keyboard-controller+1.22.7.patch` includes the two Swift
changes from [upstream PR #1608](https://github.com/kirillzyusko/react-native-keyboard-controller/pull/1608),
commit [`daf3c0081c891d442cf84cccda6ef36226276de1`](https://github.com/kirillzyusko/react-native-keyboard-controller/commit/daf3c0081c891d442cf84cccda6ef36226276de1).
The PR is open and unreviewed as of 2026-10-17. This is a trial for the reproduced
iOS composer stall when interactive dismissal interrupts keyboard opening.
It hands tracking from the display link to the existing KVO observer; it adds no
position math or private API.

The same patch carries the iOS scroll-command fix tracked in
[#542](https://github.com/EnzoTironi/zoen-native/issues/542). A command revision
travels with the target through the keyboard and composer-padding hooks. This
preserves new commands whose numeric target repeats after native search scrolling,
without replaying old commands on inset-only updates. Android keeps its existing
scroll path. The regression executes the installed package's real hooks and
animated props with substituted native host ports; native acceptance is separate.
Remove this part when an upstream release passes both repeated-target cases and
the stale-command and frozen-inset controls in `native-scroll-intent.test.ts`.

`npm install` / `npm ci` apply the patch through `postinstall`; failed application
stops installation. Include this directory in dependency cache and build-input
hashes. Native builds require development dependencies, including `patch-package`.

Before retaining the patch, verify normal opening/hiding, immediate and settled
interactive dismissal, reversing/cancelling a drag, and rotation with the keyboard
open. Remove this patch when a released dependency includes the
fix and those checks pass, or if the trial shows regressions. Recheck upstream and
patch applicability whenever changing the pinned `1.22.7` dependency.

## Markdown patch sequence

The app's existing `postinstall` runs `patch-package --error-on-fail`.
`react-native-enriched-markdown@1.1.1` uses the numeric sequence supported by
the pinned `patch-package@8.0.1`:

1. `001+native-contracts`: podspec, native props/codegen and draft contracts.
2. `002+draft-snapshot-engine`: detached snapshot and attribute restoration.
3. `003+input-lifecycle`: native mutation boundaries and input lifetime guards.
4. `004+draft-checkpoint-commands`: the complete draft-checkpoint method section.
5. `005+native-no-media`: parser admission and both native renderer/cache paths.
6. `006+public-js-bindings`: input props plus Markdown JS/types/platform gates.
7. `007+quote-source-map`: retain literal MD4C source spans through native attributed
   text and its real selection menu, including genuine RN-generated event emitters.
   Codegen uses `useLocalIncludePaths: true` to preserve the pod’s private sibling-header layout.
   Offsets use half-open UTF-16 units in the exact canonical Markdown revision.
   Unmapped layout spacers/normalization refuse the entire quote before draft mutation;
   this never substitutes rendered offsets or searches for repeated selected text.
8. `008+message-context-admission`: lets the parent message context admit read-only
   rendered body while reusing the renderer’s exact interactive-point hit test.
   Editable/selectable text, links, tasks, spoilers, images and code retain input.
9. `009+github-quote-pipeline`: route admission and source-range events through the
   actual GitHub segmented renderer selected by MessageMarkdown. Segment text
   retains original AST source coordinates; the selected segment supplies the
   native callback. Remove unused Text-only quote/admission ports, retain the common
   parser/mapper, and genuinely regenerate the GitHub event emitters.

Each patch is an authored maintained file below 20,000 bytes. They are sequential
diffs of complete package states, not arbitrary slices of a large diff. The two
input stages preserve the same original Objective-C implementation and all method
bodies. Only the complete final sequence is a buildable, qualified dependency.

The series reproduces the previous monolithic patch's complete final package
bytes, modes and paths. Qualify changes from the exact pristine 1.1.1 tarball
against `package-lock.json` integrity. Run real patch-package apply, repeat apply,
reverse, and partial/mismatched-state checks; compare the entire resulting package,
not only the edited files. Sequence bookkeeping adds `.patch-package.json`; check
that metadata separately from package source equality.

Patch-package records applied patch hashes and refuses changed applied stages.
A failed stage must exit nonzero and stop later stages. On a pristine install it
cannot infer that a disjoint patch was deleted from version control: verify all
nine expected stages and the full final source manifest during qualification.

For edits to a non-final stage, use patch-package's documented `--rebase` workflow
and regenerate successor diffs as needed. Do not hand-split hunks or substitute
the installed package for the pristine baseline. Retained generated/vendor assets
in a qualified native overlay are additional build inputs; preserve and verify
them separately instead of replacing that overlay with a source-only package.

## Temporary Automatic theme input probe (#440)

`@expo+ui+58.0.23.patch` adds only bounded native diagnostics to the existing
Toggle Binding setter and dispatch call. It does not change the binding, events,
gestures or control state. The SDK patch is silent unless the caller explicitly
sets the closed `zoen-theme-automatic-probe` testID. Zoen supplies that ID only in
builds made with `EXPO_PUBLIC_ZOEN_THEME_INPUT_PROBE=1`; normal builds are inert.
Native OSLog uses subsystem `com.zoen.theme-input`, category `Automatic`; JS logs
use `[ZoenThemeInput]`. Every fixed stage stops logging after eight observations.
No values, labels, identifiers, coordinates, event payloads or credentials are
logged or persisted. The RN touch observer is on the already-existing settings
ScrollView, so the probe does not create a new hit-test boundary around the Toggle.
A screen touch is not proof of a native Toggle hit; Binding and dispatch stages
plus JS callback/provider commit narrow that boundary. Accessibility activation
can reach Binding without a screen touch.

The existing #440 iOS failure remains unresolved. The compiled SDK JS-wrapper
regression proves the native-event protocol reaches/persists Auto, not that an
actual iPhone tap reaches that event. Native compilation and one controlled QA
input on the next opt-in artifact are required. Remove this temporary patch and
probe after that evidence identifies and verifies the actual fix. Retain the
wrapper/provider regression. `patch-package --error-on-fail` applies the patch
through the existing postinstall; no dependency version is changed.
