# Managed MLS ingress

This extends managed native custody with an initial package publication. It does not enroll an account/device, import a caller's image, open paid work or establish restore continuity.

`prepare_native_key_packages(agent, device)` generates the native facade's initial bounded batch of 32 single-use OpenMLS packages and stores their public bytes plus the matching private material in the same complete encrypted image. It activates that image through the existing device lease, staged capsule and current SQL directory admission before returning. A failed activation publishes nothing. The single bounded initial outbox is metadata within this device image, not a second authority or external state owner. It has no caller-selected batch ID, replacement or unbounded history.

`publish_native_key_packages(agent, device)` uses this exact retained batch. Signed package/device validation and the existing relay publication/hash receipts run in the same SQL transaction as the current native directory/device/profile locks. The helper requires a SQL transaction rather than a raw autocommit connection. Stock overflow or failure rolls back the whole SQL operation. Only a known successful commit permits retention of the native acknowledgement.

SQL publication and image acknowledgement are separate durable steps. A crash or failed/unknown result between them leaves the initial native batch pending. Retrying publishes the same bytes; the existing SQL hash receipts prevent reinserting a package that has already been destructively claimed. A stale worker cannot replace a newer native root. Once the image has retained its acknowledgement, further calls return that historical record and generate no new packages or SQL stock. This record is not evidence of current stock, receipt delivery or deployment continuity. Pending expired or invalid packages refuse publication; the API does not replace them silently.

After an authorized owner has added the device using a genuinely claimed package, the existing `sync_reply_runs` reads the configured `RuntimeSource`, processes ordered Commit/Welcome/Application through DeviceCore and activates the exact complete image. No caller-provided event batch or frontier crosses this boundary. Sync/discovery may retain inert run identifiers while paid dispatch remains closed.

## Deployment restore is a separate operation

Do not start or resume managed publication on restored SQL, FDB or capsule snapshots as though this were ordinary process reopen. Use a new authenticated authority/device with reenrollment and rejoin, or independently proven external continuity, under [the separate restore contract](https://github.com/EnzoTironi/zoen-native/issues/327). Matching local markers and SQL publication hashes cannot detect simultaneous rollback of their own stores. No API boolean or marker here claims to be that witness. Unknown restore is not an authorized activation mode for this increment.

## Bounds and limitations

This first delivery publishes single-use initial packages only. Automatic stock refill, last-resort packages, public WebSocket enrollment and operator provisioning are separate work. Profiles/certificates must already be registered and the real device/capsule secrets must already exist in the configured vault. A missing retained batch/key, invalid image or revoked device is not permission to regenerate or reset state.

The runtime regression uses its normal managed constructor, a new file-backed fixture vault, real SQL receipt publication/claim and a real OpenMLS owner join through the actual FDB log. It covers pre-activation refusal, competing preparers, a known SQL commit with ACK discarded, cold retry after consumption, real Welcome/Commit/Application and cold discovery. The fixture is not deployed KMS, actual wire-level commit-unknown, process-death recovery or production service evidence. External nonrollback continuity, model dispatch, production output and refund-by-absence remain closed.
