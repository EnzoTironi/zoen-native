# Managed native custody

`RuntimeAuthority::open` retains its closed defaults. Services that already hold an enrolled agent device can use `open_with_native_device(config, vault, agent, device, key_version)` to load native custody. This is not device enrollment or deployment of a KMS.

The vault implements the same `client_core::SecretVault` contract as clients. Provisioning is outside this constructor. `native_capsule_vault_key(namespace, version)` names a separate 32-byte capsule key; `native_device_vault_key(namespace, agent, device)` names the enrolled device's 32-byte signing secret. Locators are opaque, domain separated hashes, not authority. The constructor never writes, deletes, generates or substitutes secrets. Do not pass a root identity secret as a device secret.

Public profiles, owner relationships and the device certificate come from the SQL directory. Existing profile signatures, certificate validation, device possession and current non-revoked registration must all agree. The existing immutable SQL/FDB deployment pairing also applies. An existing capsule must authenticate with the retained key and restore through DeviceCore before open succeeds.

`initialize_native_device` creates only an empty DeviceCore capsule through the existing FDB device lease, encrypted chunk staging, fresh SQL directory admission and bounded activation capability. No conversation, caller-supplied image, grant or root secret is imported. It does not generate or publish MLS key packages. A retry observes the existing capsule instead of resetting its state or incrementing its generation. `inspect_native_device` and `repack_native_device` use their existing current-directory checks.

SQL device revocation is the running service's authority fence. Deleting a vault entry prevents a new open but does not erase copies already loaded in process memory. Retained keys and their backup/rotation policy remain the vault operator's responsibility. Changing the version or key without a separate authenticated migration cannot adopt the old capsule.

## Gates that remain closed

The constructor does not establish restore continuity or open model dispatch, cancellation, production output, relay currentness or device enrollment. Continuity remains false and the production output scope remains unavailable. Feeding MLS events, joining authorized conversations and reconciling retained execution across restores require their own real authority and evidence.

## Regression coverage

The runtime's real SQL/FDB journey exercises the public constructor with a fresh file-backed fixture vault: missing/wrong secrets, foreign deployment namespace, empty initialization, lost-response retry, repack, cold runtime reopen, ciphertext inspection, another device and revocation. The fixture adapter is not evidence of a deployed KMS, process-crash recovery or paid model delivery. The existing native activation suite separately covers lease/admission and failure cuts.
