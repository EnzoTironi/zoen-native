> Historical standalone-client guide. Current application setup is in
> [the Expo README](../../apps/zoen/README.md). Source paths below refer to
> [the archived native clients](https://github.com/EnzoTironi/zoen-native/tree/5149ae48be3b104fbe23b4e6a91bef80f131844c).

# Apple attachments and push

Updated 2026-10-12. This document records the native media unit and its remaining Core and provider dependencies.

## Sending an attachment

`ChatAttachmentButton(spaceId:)` owns selection, review and sending. The conversation composer offers Camera on iOS, Photos and Files. A private copy is reviewed before Send. Camera capture returns actual JPEG bytes. A cancelled selection sends no event. A failed send leaves the remaining files in review; successful files leave the retry queue.

Send calls the existing `fileAdd(spaceId:path:name:mime:bytes:thumbnail:)`. Core signs the File Item, derives chunk secrets with HKDF-SHA256, encrypts chunks with XChaCha20-Poly1305 and queues uploads. There is no separate attachment endpoint. This API has no thread identifier, so the thread composer has no attachment button.

| Boundary                   | Native limit or behavior                                                                                                    |
| -------------------------- | --------------------------------------------------------------------------------------------------------------------------- |
| File                       | 32 MiB, nonempty, regular file                                                                                              |
| Selection                  | 10 files, 64 MiB combined                                                                                                   |
| Raster image               | 64 million pixels, 16,384 pixels per dimension                                                                              |
| Review thumbnail           | Downsample to at most 480 pixels per dimension                                                                              |
| Other Quick Look thumbnail | Request 240 points at scale 2; reject a result over 512 pixels per dimension or 2 MiB encoded                               |
| Supplied filename          | Strip directory components and control characters; cap at 200 UTF-8 bytes                                                   |
| Provider URL               | Hold security-scoped access while making the private copy                                                                   |
| File descriptor            | Open with `O_NOFOLLOW`, `O_NONBLOCK` and `O_CLOEXEC`; require a regular file via `fstat`; read and copy the same descriptor |

Selection reads in 1 MiB pieces and checks limits again while reading. Import tasks propagate cancellation. Cancel, dismissal, successful sends and selection removal explicitly discard the draft's decrypted copies. Quick Look, markup and sending hold separate use leases. Discard removes a copy after its last active use closes, even if SwiftUI still retains its sheet closure or state. Quick Look releases its lease in the representable's dismantle callback; the Mac preview is also explicitly closed. Markup copies the edited provider file before the provider can remove it. Deinitialization remains a cleanup fallback.

Each process holds an exclusive lease in its private staging session under the system temporary directory. Session directories use mode 0700 and files use 0600. They are excluded from backup and use complete file protection on iOS. A new staging session removes only abandoned `session-UUID` directories with the exact lease marker. It does not remove sessions whose lease is held by another process, symlinks or unrelated directories. Publishing the session happens after acquiring its lease. Crash cleanup runs when a subsequent attachment operation opens a staging session.

## Receiving and historical previews

The currently generated Core216 Swift API has `fileBytes` but no caller-selected allocation cap. Native latest-version preview first checks the observed item version and file byte count, then requests that same version. Historical previews remain blocked in this unit because their metadata and allocation bounds cannot be verified through the current Swift API.

The Core source adds these methods, pending its tests and regenerated bindings:

```text
fileBytesBounded(itemId: String, version: UInt32?, maxBytes: UInt64) -> Data?
fileInfoAt(itemId: String, version: UInt32?) -> FileDto
```

The bounded assembler must validate chunk counts, declared sizes, actual cached sizes and thumbnail limits before a full allocation. Native preflight alone cannot enforce that boundary for an underdeclared remote manifest. Reception acceptance and historical preview acceptance remain pending until the new ABI is built and the native callers use it.

## Push contract and remaining work

The app currently has notification permission requests. `PushCoordinator` now defines the native lifecycle against `PushNotificationSystem` and `PushRegistrationStore` protocols, with no installed OS/Core adapters or app entrypoint changes. There is no Core push registration ABI yet. Permission alone is not delivery readiness. Initial native registration is absent; every later snapshot comes from the store, including unsupported or refused states. The coordinator never constructs a queued or registered snapshot.

The agreed Core registration contract is pending implementation and bindings:

```text
setPushRegistration(token: String?, environment: String, topic: String) -> PushRegistrationDto
pushRegistration() -> PushRegistrationDto
```

The snapshot carries `supported`, `state` as `unregistered`, `queued`, `registered` or `refused`, `environment`, `topic` and `deliveryAvailable`. Persisting a token locally may produce `queued`; only a relay acknowledgement may produce `registered`. Provider credentials gate `deliveryAvailable`. A nil token is a durable removal. Registration is idempotent and bound to the authenticated device. Network transmission requires the negotiated `push_registration_v1` capability from the existing Challenge intersection.

The coordinator handles permission and APNs callback input, forwards canonical tokens idempotently, reads Core snapshots and discards callbacks after a device change or logout. Default retries wait 10, 20, 40, 80 and 160 seconds, then stop. Token state stays in memory. Denied permission and detach request a nil-token removal; detach returns false if persistence failed and still invalidates asynchronous callbacks. Calling detach before erasing the Core device is required for its removal request to be durable.

`PushWake` decodes the source-backed [wake contract](push-contract.md). It accepts `aps.content-available` as integer 1 and `zoen` with integer `v: 1`, an ASCII `space_id` of 1 through 128 letters, digits, underscores or hyphens, and a nonzero `event_seq` as a canonical decimal UInt64 string. Boolean or floating values, numeric cursors, unknown versions, extra fields and payloads over 4096 bytes are rejected. Cursors above 2^53 and UInt64.max stay exact. The decoder produces only a sync hint. The store must authenticate the current device and validate membership and the signed log before reporting synchronized data. Receiving this background payload never navigates, approves requests, executes actions or creates a tappable notification. AppDelegate wiring, Core adapters, registration transport and provider delivery remain pending.

Apple's entitlement uses `development` or `production`. Core accepts `sandbox` or `production`, so native maps the signed `development` entitlement to `sandbox`. The Core input contract accepts 1 through 256 token bytes as even-length hexadecimal and normalizes it to lowercase. APNs tokens have variable length. The app must request a fresh token each launch and forward the returned bytes without assuming a fixed length. See [APS environment](https://developer.apple.com/documentation/bundleresources/entitlements/aps-environment), [registration callback](<https://developer.apple.com/documentation/uikit/uiapplicationdelegate/application(_:didregisterforremotenotificationswithdevicetoken:)>) and [APNs request token encoding](https://developer.apple.com/documentation/usernotifications/sending-notification-requests-to-apns).

The Personal Team Device configuration still excludes paid push capabilities. Provider credentials, APNs signing and physical-device delivery proof are separate remaining gates.

The new decoder/coordinator boundary journeys use isolated adapter fixtures:

```sh
xcrun swiftc -swift-version 6 -parse-as-library \
  apple/Shared/Sync/PushWake.swift \
  apple/Shared/Sync/PushCoordinator.swift \
  apple/Tests/PushWakeJourney.swift \
  -o /tmp/zoen-push-wake-journey
/tmp/zoen-push-wake-journey
```

They cover strict wake types and exact cursors, signing/topic/token bounds, truthful unsupported/queued/registered store snapshots, nil-token removal, stale device/logout callbacks, denied membership, bounded retries and a failed unregister write. Compilation and these five journeys await the coordinator's slot. Their fixture snapshots are unit inputs, not evidence of APNs registration or a real relay acknowledgement.

## Verification

Run the owned filesystem journeys from the repository root:

```sh
xcrun swiftc -swift-version 6 -parse-as-library \
  apple/Shared/Features/Files/AttachmentStagingSession.swift \
  apple/Shared/Features/Files/AttachmentFile.swift \
  apple/Tests/AttachmentFilesJourney.swift \
  -o /tmp/zoen-attachment-files-journey
/tmp/zoen-attachment-files-journey
```

On 2026-10-12, all six baseline journeys passed before the explicit dismissal fix. They exercise provider-copy isolation and owner lifetime, supplied paths and nonblocking FIFO/symlink rejection, per-file and selection limits, real JPEG downsampling and an oversized valid compressed TIFF, import cancellation, and a real owned subprocess crash followed by cleanup. The crash journey also preserves a second live process's staging directory, the current preview and an unrelated directory. A seventh journey now retains sheet/send closures while discarding their copies, holds two preview leases, and checks removal only after the last preview closes. That added journey and the changed source await the coordinator's test slot. Fixtures use isolated UUID temporary paths; the test kills only its own child process.

The baseline sources passed Swift parsing and `git diff --check`. The coordinator's v21 native run passed three Debug builds against retained Core216. In the actual Mac journey, a 119-byte fixture opened in review and Quick Look displayed its exact Unicode text. Back returned to review and Cancel closed the sheet with composer focus. The cleanup check failed: its private review copy was still present about 20 seconds after cancellation. That failure triggered the explicit discard/use-lease change above; its runtime rerun is pending. The picker automation initially lost the modal's AX binding; selecting the exact app and raising its sheet restored access, with no observed app crash. UI evidence must be attached to an existing UI/UX pull request with `gh --attach`. These checks do not establish independent-device encrypted transfer, historical preview acceptance or push delivery.

## Final handoff checks

At the final frozen checkpoint, Root compiled and ran the updated attachment executable: seven journeys passed, including explicit discard with a retained closure and two independent preview leases. Root also compiled and ran the push executable: five boundary journeys passed. Swift grammar passed for 17 current files. The final full Mac app build did not start because its unchanged 1 GiB disk preflight failed; v21 app/UI evidence remains earlier scope. The actual cancel-cleanup UI failure still needs a rerun. See [the complete handoff](handoff-2026-10-12.md).
