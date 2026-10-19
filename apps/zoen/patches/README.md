# Dependency patches

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
open. Remove it and its installation hook when a released dependency includes the
fix and those checks pass, or if the trial shows regressions. Recheck upstream and
patch applicability whenever changing the pinned `1.22.7` dependency.
