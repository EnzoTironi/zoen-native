# APNs wake contract

Source checkpoint: 12 October 2026, shared `main`. The captured `roda-proto::push` source defines validated APNs inputs and a concrete sync-wake encoder. Its four new tests passed within the complete 28-case protocol library run. The later complete Core run was interrupted when the module entry disappeared from `roda-proto/src/lib.rs`; that source guard failure is preserved, and the entry must be restored before accepting the current source. This unit does not implement registration transport or provider delivery, and the shared capability list does not advertise `push_registration_v1` yet.

The exact payload from `encode_sync_wake(space_id, event_seq)` is:

```json
{
  "aps": { "content-available": 1 },
  "zoen": { "v": 1, "space_id": "sp_01jqtest", "event_seq": "18446744073709551615" }
}
```

`zoen.v` is the integer `1`. `space_id` is a nonempty ASCII identifier of at most 128 bytes, containing only letters, digits, underscores and hyphens. `event_seq` is a nonzero `u64` encoded as a decimal string to preserve exact cursors through JSON consumers. The payload carries no content, action, approval or credentials. A client treats it as a hint and still validates account ownership, Space membership and the signed log before using fetched data. The decoder must reject unknown versions and invalid field types before entering the existing Space-validation route.

The eventual provider request must set `apns-push-type: background` and `apns-priority: 5`. The encoder deliberately includes no alert, sound or badge. Background refresh is best effort under [Apple's background-push contract](https://developer.apple.com/documentation/usernotifications/pushing-background-updates-to-your-app); successful registration would not guarantee notification delivery.

`PushEnvironment` accepts exactly `sandbox` or `production`. `ApnsToken` accepts nonempty, even-length hexadecimal input, normalizes it to lowercase and caps it at 512 ASCII characters (256 decoded bytes). This is an explicit Zoen input bound, not a claim that APNs tokens always contain 32 bytes: [Apple documents variable token lengths](https://developer.apple.com/documentation/uikit/uiapplicationdelegate/application(_:didregisterforremotenotificationswithdevicetoken:)). Debug formatting and validation errors do not expose token contents. The native caller should hex-encode the actual `Data` supplied by APNs without fixed-size truncation.

Topics must be nonempty bundle-style identifiers, at most 255 bytes, with nonempty dot-separated components containing ASCII letters, digits or hyphens. Wildcards, whitespace and path characters are refused. Syntax validation does not supply an allowlist: the relay registration implementation still needs an explicit configured topic allowlist and must refuse missing configuration.

Before exposing `setPushRegistration` or advertising support, the remaining unit must implement authenticated current-device mutations, durable acknowledgements, revision checks, retry IDs, unregister tombstones and revocation fences through the SQL commit. Token data must stay out of returned DTOs and support exports. `deliveryAvailable` stays false until an actual configured provider implementation is available. No APNs credentials were configured and no provider request was sent in this source unit.

After the coordinator reserves a frozen-source test slot:

```sh
cargo test --locked -j2 -p roda-proto push::tests -- --test-threads=1
```

That command checks the payload/input contract only; it does not certify end-to-end delivery or registration.
