# Dependency patches

`@expo+inline-modules+0.2.6.patch` fixes the SDK 58 watched-directory lookup after
CocoaPods' `xcodeproj` serializer quotes the same `SOURCE_ROOT` path. It uses the
installed Expo Xcode utility to compare the path's value and records newly created
groups immediately. It changes no dependency version. The existing native-surfaces
config plugin separately reconciles aliases generated before the patch, preserving
the original UUID, exceptions and memberships or refusing conflicting metadata.
See [the reproduction and CNG contract](../../../docs/dev/expo-inline-modules-idempotence.md).
Requalify the pinned patch when upgrading this package and remove it when the
released updater passes the same quoted-path and Ruby serialization regressions.

`react-native-keyboard-controller+1.22.7.patch` backports exactly the two Swift
changes from [upstream PR #1608](https://github.com/kirillzyusko/react-native-keyboard-controller/pull/1608),
commit [`daf3c0081c891d442cf84cccda6ef36226276de1`](https://github.com/kirillzyusko/react-native-keyboard-controller/commit/daf3c0081c891d442cf84cccda6ef36226276de1).
The PR is open and unreviewed as of 2026-10-17. This is a trial for the reproduced
iOS composer stall when interactive dismissal interrupts keyboard opening.
It hands tracking from the display link to the existing KVO observer; it adds no
position math or private API.

`npm install` / `npm ci` apply the patch through `postinstall`; failed application
stops installation. Include this directory in dependency cache and build-input
hashes. Native builds require development dependencies, including `patch-package`.

Before retaining the patch, verify normal opening/hiding, immediate and settled
interactive dismissal, reversing/cancelling a drag, and rotation with the keyboard
open. Remove this patch when a released dependency includes the
fix and those checks pass, or if the trial shows regressions. Recheck upstream and
patch applicability whenever changing the pinned `1.22.7` dependency.
