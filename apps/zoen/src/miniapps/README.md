# Device capabilities

`native-capabilities` is the platform boundary for the MCP Apps View host. The host
owns manifest checks, Rust device grants, user consent, account checks, and the
separate confirmation before sharing a photo. The adapters do not persist grants
or write shared item state.

| Capability                          | iOS                                                       | Web                                        |
| ----------------------------------- | --------------------------------------------------------- | ------------------------------------------ |
| `photos.pick`                       | System image picker, selected images only                 | Browser file picker                        |
| `camera.capture`                    | System camera, physical device                            | Live camera with capture/cancel controls   |
| `location` / `location.approximate` | Foreground permission, one fix                            | Secure-context browser permission, one fix |
| `calendar.freebusy`                 | Full calendar read permission, merged busy intervals only | Not advertised                             |
| `calendar.add`                      | Write-only permission, system editor, saved result only   | Not advertised                             |
| `contacts.pick`                     | Selected contact name only                                | Contact Picker API when available          |
| `calendar.events` / `health.steps`  | Not advertised                                            | Not advertised                             |

Native adapters detect modules compiled into the current binary before advertising
them; adding an npm package alone does not enable its feature in an older dev build.
Android adapters are deferred. Calendar uses SDK58's new class API. Contacts uses
the explicit `expo-contacts/legacy` picker: SDK58's new picker returns an ID and its
name getter re-reads `CNContactStore`, which would require broader contact access.
The compatibility path receives the selected contact directly, requests no address
book permission, and passes only its name and initials to the miniapp.

Photos are decoded with SDK58's bounded image load, re-encoded as fresh JPEG pixels
without source EXIF/GPS, and reduced until each data URL fits the core's 150,000
character limit. Source images have a 32 MiB limit; one call selects at most six.
Previews use cryptographic opaque tokens bound to the owning item, expire after
15 minutes, and are bounded to 12 per item / 24 total. Native picker/manipulator
cache files and browser Blob URLs are released after processing. Tokens and image
previews remain process-only. The host must call `releaseItemPhotos` on teardown;
this also invalidates pending selection/location work.

Location stops after one fix, cancellation, backgrounding, or a 12-second timeout.
Approximate fixes snap coordinates to a 0.02-degree grid and report at least 3 km
accuracy. Calendar reads are limited to 31 days and never expose titles, calendar
IDs, attendees, or all-day/free events. Browser dialogs supply a fresh user gesture
for file/contact pickers and release camera tracks on close or backgrounding.

`capability-data.test.ts` verifies token isolation/expiry/teardown, limits, merged
freebusy intervals, event validation, approximate location, and contact minimization.
These tests and TypeScript/lint checks do not prove OS permission dialogs, camera
hardware, calendar integrations, or browser-specific pickers. Those paths still
require runtime acceptance checks on the rebuilt app and supported browsers.
