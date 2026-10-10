# Profiles: the core API for the app

For the profile screen on `ui/*`. Everything below is in the Rust core (`roda-ffi`) and
generated into Swift by UniFFI (`RodaCore`). Design: [ADR 0016](adr/0016-encrypted-profiles.md).

Use [roadmap status](roadmap-status.md) for the current platform/version ledger. This API contract describes profile operations; it does not establish that every client and public deployment is on the same protocol.

## What a profile is
Name, bio (status line) and an optional photo. They're encrypted on the device, and the
relay stores only ciphertext. Contacts are people you share a chat, group or Space with;
they get your profile key automatically and see everything. Everyone else, such as search
results or invite previews from strangers, sees only the @handle.

## Calls

```swift
// Anyone's profile, as this device can read it (your own included).
let p: ProfileDto = try core.getProfile(identityId: id)

// Edit yours. Name is required (≤ 64 chars), bio ≤ 280, photo ≤ 5 MB (JPEG, PNG, HEIC, WebP).
let me = try core.updateMyProfile(name: "Ana", bio: "Trilhas e café", photo: .keep)
try core.updateMyProfile(name: "Ana", bio: "", photo: .set(bytes: jpegData, mime: "image/jpeg"))
try core.updateMyProfile(name: "Ana", bio: "", photo: .remove)

// Stop sharing your profile with someone (rotates your key; they keep what they already saw).
try core.blockPerson(identityId: id)
try core.unblockPerson(identityId: id)
let blocked: [String] = try core.blockedPeople()

// The photo bytes, once `photoReady` is true:
let data = try core.media(sha256: p.photoSha256!)
```

`updateMyProfile` returns at once. The encrypted upload, the photo blob and the key shares
go out over the sync connection, from the outbox when you're offline.

```swift
struct ProfileDto {
    var identityId: String
    var handle: String          // without "@"
    var name: String?           // nil = not a contact: show "@\(handle)"
    var bio: String?            // nil = not a contact; "" = no bio
    var photoSha256: String?    // nil = no photo (or not a contact)
    var photoReady: Bool        // bytes are on this device (else still downloading)
    var version: UInt64         // 0 = nothing readable yet
    var tintHex: String         // monogram background when there's no photo
    var isMe: Bool
    var blocked: Bool
}

enum PhotoChange { case keep, remove, set(bytes: Data, mime: String) }
```

## Change events
`CoreListener` gained one callback:

```swift
func onProfileChanged(identityId: String)   // name, bio or photo changed on this device
```

`SyncBridge` forwards it to `SyncModel.profileRevisions[identityId]`, a counter the profile
screen can observe to re-read `getProfile`. It also triggers the usual `app.refresh()`, so
chat titles and author names update everywhere. A photo arriving after its profile fires the
callback a second time, with `photoReady == true`.

## Display rules
- Show `name ?? "@" + handle`; the handle is always shown under the name.
- Personas and chat titles already carry the readable name, falling back to `@handle`.
  `Persona.name` is `@handle` for people whose profile this device can't read.
- `isMe` → show the edit affordance; `blocked` → show "Unblock".
- Agents are not encrypted: `name` and `bio` come from their public directory entry.

## Errors
`CoreError.Invalid` with a localized `reason` for an empty name, a long name or bio, or a
bad photo type or size. `CoreError.NotFound` for an id this device has never seen.

## Wiring today's UI
- `SyncModel.updateProfile(name:handle:bio:)` (the edit sheet) already writes name and bio
  into the encrypted profile through `core.updateProfile`. A changed @ also re-registers.
- `AvatarPhotoStore` keeps a photo only on this device. To share it with contacts, pass the
  cropped JPEG to `updateMyProfile(..., photo: .set(bytes:mime:))`, and render other people
  from `getProfile(identityId:)` plus `media(sha256:)` once `photoReady`.

## CLI equivalents (for testing against a relay)

```
zoen profile set --name Ana --bio "Trilhas e café" --photo ana.jpg
zoen profile show @ana [--out photo.jpg]
zoen block @bruno | zoen unblock @bruno
```
