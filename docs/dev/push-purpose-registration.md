# Purpose-scoped push registration

Issue #298 separates ordinary notification tokens from VoIP tokens. It does not
enable incoming-call wakes or declare cold/background calls ready.

The authenticated `RegisterPush` operation retains its existing wire shape:

| Platform | Server-derived purpose | APNs environment |
| --- | --- | --- |
| `apns` | `notification` | production |
| `apns-sandbox` | `notification` | sandbox |
| `fcm` | `notification` | not APNs |
| `apns-voip` | `voip` | production |
| `apns-voip-sandbox` | `voip` | sandbox |

An empty token clears only that device's matching purpose. Migration 0034 keeps
all existing tokens as `notification`, then changes the key to
`(identity, device, purpose)`. Refreshing, clearing or moving a token within one
purpose leaves the other purpose intact. Provider death is purpose-scoped too.
Both lookups join the current unrevoked device row. Existing session admission
and delivery authorization remain in force.

The ordinary message wake only looks up `notification`. `Pusher::wake` refuses
VoIP platforms. The separate `Pusher::call_wake` transport uses the same configured
APNs signing key with topic `<ZOEN_APNS_TOPIC>.voip`, push type `voip`, priority
`10` and an explicit short expiration. Its bounded payload contains an opaque
128-bit hexadecimal wake nonce and expiry, with no caller, conversation, message
content or real call ID. Provider delivery rechecks expiry. Tokens and nonce are
omitted from diagnostic formatting.

No session operation invokes this transport, no generic publication routes to
it, and no VoIP/cold-call capability is advertised. Before adding that admission
path, the receiver's call/block policy must be enforced. The relay cannot read
CallV1 within MLS, and local blocked-person state is encrypted on the client.
An opaque PushKit hint alone cannot distinguish a blocked caller before reporting
CallKit. See #296 for cold adoption and #178/#266 for the full acceptance scope.

Apple's [APNs request contract](https://developer.apple.com/documentation/usernotifications/sending-notification-requests-to-apns)
defines provider headers. Its [PushKit guidance](https://developer.apple.com/documentation/pushkit/responding-to-voip-notifications-from-pushkit)
and [E2EE VoIP guidance](https://developer.apple.com/documentation/callkit/sending-end-to-end-encrypted-voip-calls)
must guide the eventual receiver policy and supported-OS behavior. This increment
does not demonstrate APNs delivery, signing/entitlements or a physical iPhone.

Automated coverage includes protocol bounds/redaction, actual provider request
construction via the existing HTTP seam, generic-versus-VoIP routing, legacy SQL
migration, coexistence, refresh, clear, token transfer, provider death and revoked
device lookup. The SQL integration test uses a fresh test-owned database and the
production directory helpers; CI supplies `ZOEN_TEST_PG`.
