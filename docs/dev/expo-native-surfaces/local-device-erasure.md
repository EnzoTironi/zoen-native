[Native surface index](../expo-native-surfaces.md).

## Local device erasure

The You screen's **Sair e apagar dados** action captures the current identity and
device before displaying its destructive confirmation. The store rejects an old
confirmation after an account change and immediately fences old UI results,
timelines, live status and new requests while the operation runs.

Before deleting canonical data, it awaits the registered projection cleanup: widget
content, entity catalogs, activity termination, queued App Intents,
presentation preferences and device notifications. Notification cleanup drains local
notification and badge writes already in flight. If any cleanup fails, the operation
stays retryable and does not delete the account's keys or database. A process restart
at that preflight stage may restore the unchanged account; erasure has not committed.

The iOS bridge then records a small nonsecret intent scoped to the exact device,
drains active core requests and history reception, stops sync and calls the existing
Rust erasure operation. It additionally verifies removal of the account's complete
Keychain namespace and clears the six owned cache namespaces: `ZoenMedia`,
`ImagePicker`, `ImageManipulator`, `DocumentPicker`, `ExpoAudio` and `zoen-exports`.
The preflight also drains active SDK file writers and clears image memory/disk caches
through the public Expo Image APIs. Private image views disable caching. Picker
originals are disposed after profile processing, including failure paths. Cleanup
preserves photo-library originals, Documents, assets, fonts and update caches; re-audit
the owned namespace list when upgrading the SDK packages. Missing files are distinguished
from failed metadata inspection: inaccessible cache or journal parents fail closed instead
of being treated as empty. The intent is removed last; a cold
open resumes an interrupted destructive stage before allowing another account to
open. Neither the account on other devices nor their histories is deleted.

The browser driver uses a strict, atomic IndexedDB transaction to clear the complete
protected vault while retaining its exclusive account lock. It aborts provider setup,
stops messaging and pairing, and releases staged files and photos. A failed storage
transaction remains fenced and retryable; a successful clear removes device keys,
session and root authority together. The action is not offered on Android until its
native lifecycle has separate implementation and acceptance.

Store, browser-vault and pure Foundation tests cover ordering, stale ownership,
delayed writes, inaccessible files and retry behavior. Signed iOS checkpoint `f9544c3a`
passed two disposable-device confirmations, cold onboarding without Metro and exact
primary-history preservation; it also accepted profile save/cold reopen and canceled
silent recording. Actual Web checkpoint `03d787f` passed erasure and peer preservation
in CI. Signed v3 and v4 installed acceptance subsequently passed with the
cache-inspection correction: confirmed local erasure, cold onboarding without Metro
and exact primary-history preservation. v3 repeated cancel as well. A sanitized
runtime witness found that the official SDK launch envelope reaches the native
intent with a slash root. The minimal validator correction accepts only that root
variant while retaining the exact SDK query checks and never following its inner
address. Signed v4 opened directly into Chats through the official CLI and through
a Metro-off linked cold restart; it then passed erasure and Metro-off empty cold
onboarding. Its 244-input source SHA256 is
`f53a592b3dd40bfe6c9d52c99efa8b7cc8f6c2dc40a66b0bcd353087d13d67b0`,
with JS bundle SHA256
`9b23c6324db740ca431ecdd00c34676bf8a7e95ae51109e1f2fcef89e9288553`.
Temporary diagnostic instrumentation was removed before the final bundle.
Existing personal accounts must never be reset to produce evidence.
