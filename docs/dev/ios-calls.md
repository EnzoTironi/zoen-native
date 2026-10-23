# Private iOS calls

Tracking: [#266](https://github.com/EnzoTironi/zoen-native/issues/266), part of #178.
This is implementation work, not accepted calls or a release claim.

## Authority and transport

`CallV1` is a closed event in the existing signed Space log/SQLite/outbox. Only a
certified device in a two-person, end-to-end encrypted DM can invite or answer.
Responses bind the invitation's signed-content hash, which survives confirmation.
The first canonical acceptance chooses one callee device. An optimistic local
acceptance is presentation state, not permission to send media negotiation.
Cancellation/acceptance order and terminal history replay through the same Core.
Membership changes retire the call. Invitations ring at most 60 seconds; future or
expired unanswered invitations cannot permanently block subsequent invitations.

`CallTransportV1` carries SDP and ICE inside MLS. It is restricted to the caller
and selected callee devices, and only after invitation and acceptance confirmation.
SDP/ICE never use clear relay ephemera. Stable signal IDs make ambiguous retries
idempotent; reusing an ID for different bytes is refused. The Core bounds each SDP
to 48 KiB, ICE candidate to 4 KiB, call to 96 signals/512 KiB and eight negotiation
generations. There is one caller offer and callee answer per generation. These are
our declared initial bounds, not constants copied from Signal.

WebRTC provides DTLS/SRTP. Its fingerprints are delivered in the authenticated MLS
SDP; Zoen adds no media cryptography. The initial ICE policy is relay-only, avoiding
disclosure of the participants' host/public IP addresses. TURN credentials are a
bounded transport capability, not authority to accept a call. There is no public
STUN fallback. Coturn configuration must deny private/loopback/link-local and
multicast peers, restrict allocations/rates, and keep its REST secret server-only.
Credential expiry alone does not evict an existing TURN allocation.

## Pinned maintained engine

The native adapter uses the official Jitsi build used by React Native WebRTC:

- Jitsi WebRTC `155.0.1`, upstream tag `v155.0.1`.
- [Official artifact](https://github.com/jitsi/webrtc/releases/download/v155.0.1/WebRTC.xcframework.zip),
  252,220,535 bytes; SHA-256
  `5189cc5864d917400174f5bcee3a62f27c55524a28bccec783575d5efbda2ae5`,
  also published in the tag's `Package.swift`.
- `ZoenWebRTC.podspec` pins URL and SHA-256. The Expo plugin adds it as a
  CocoaPods `:podspec` source, while autolinking lists only the source module.
  Autolinking a local binary spec as `:path` would ignore its download source. Its BSD-3-Clause notice is retained in the spec. Distribution must also
  include the notices of the compiled upstream artifact's bundled dependencies.
- No React Native WebRTC wrapper, engine fork, Signal app fork or LiveKit SFU is
  installed. The adapter owns one native peer and reuses the existing Core session.

The real ZIP was downloaded and checked locally. The simulator framework was used
to typecheck the actual RTC, CallKit, and native view sources against SDK iOS26
with Swift6 strict concurrency and warnings as errors. This is not an application
link/build, runtime media test or physical-device proof.

## Native ownership

One process-wide `CXProvider` owns system call presentation. A transient coordinator
borrows an exact account/device/session generation through `ZoenCallPort`. Its
transport callbacks revalidate that scope; it never opens another Client or vault.
Mic starts only after authenticated acceptance, OS permission and CallKit activation.
A system accepted action does not mark media connected: only WebRTC's combined
ICE/DTLS connected callback does. End stops capture and the peer before awaiting a
canonical terminal write. Its audio-only release closure does not retain the Core
Client while waiting for CallKit deactivation. Retired callbacks cannot alter a replacement call.

The media implementation includes relay-only ICE, authenticated SDP exchange,
remote video, mute/speaker/camera controls, bounded caller ICE restarts, and native
CallKit actions. A continuous 30-second failed/connecting resource interval ends
through the normal signed terminal path, including on the callee device. Only
actual connection cancels that deadline. Session ports use the Core TURN API with
account/device checks before and after its await. A fresh binding or next outgoing
attempt retires this device's abandoned accepted call with a signed hangup, or its
abandoned outgoing invitation with cancellation; it never revives media after crash.
Ending/membership changes discard derived SDP/ICE payloads while retaining durable
canonical history. Reader-role changes retire media as well.

The conversation exposes a native Expo menu: tap for voice, hold for voice/video.
A bounded native ticket hands audio ownership over from existing Expo playback and
recording. Captured voice drafts are recovered before acknowledging the handoff;
old audio mode changes drain, later ones are fenced, and Expo audio is deactivated
before CallKit acquires it. End stops capture, waits for system deactivation and
releases the exact ticket. Existing players remain paused; a new explicit playback
or capture action reactivates Expo audio. A missing JS acknowledgment times out in
five seconds and does not admit microphone capture. SDP/ICE credentials remain native;
the JavaScript call UI receives presentation status, not transport secrets.

## Current evidence and remaining acceptance

The canonical signaling slice is Draft #278. Rust tests cover signed events and
SQLite/cold replay, multi-device acceptance, accept/cancel order, expiry, future
clock recovery, malformed references, closed encoding, invalid participants and
stale-owner no-write behavior. The transport slice adds tests for confirmation
before media, selected-device scope, retry/replay, terminal refusal, negotiation
bounds and normal signed retirement of a cold orphan. Report actual CI outcomes
on the issue; written tests are not execution evidence.

The first wiring cohort passed TypeScript and actual RTC/CallKit sources passed
Swift6 strict-concurrency typecheck against the real upstream SDK. The audio mode,
recording and playback regression group passed 74 tests; the real Swift ticket
implementation also passed stale-ticket, rejection, retirement and real expiry
cases. The first broad app run passed 1,796 of 1,801 tests; its five failures were
new unregistered call ports in the chat-chrome fixture. The corrected actual
component test passed all six chat-chrome cases, including voice/video menu and
availability. Broader rerun and Core transport results belong on the issue.

The actual host Core was built and genuine UniFFI0.32.2 Swift bindings were
emitted from it. All135 ZoenCore Swift sources plus those bindings typechecked
against real existing Expo/dependency modules and the pinned WebRTC SDK. A linked
host Swift-to-Rust probe exercised all four new native call methods and confirmed
stale owners/malformed negotiation remain refused. This is ABI/typechecking evidence,
not an iOS app link or a call. The final privacy/retirement tests exercise the actual
transient Swift resource models; they do not execute real camera/CallKit hardware.

Own call and transport ACKs use the existing canonical replay gate. Real optimistic
append-to-Sequenced tests cover both confirmation flags and canonical device choice
against a competing local accept. A queued cancel losing to a canonical accept is
retired by normal signed hangup once the abandoned call is observed again. Privacy
intent is installed before RTC creation and gates delayed permission/capture results.
The owner-specific retirement fence is installed before cleanup awaits; deactivation
and reset cannot be lost while stop or terminal publication is suspended.

Still open: implement PushKit registration/report/cold handover with the same process gate;
build the integrated app once; prove real two-client TURN
media, reconnection and revocation; then test the physical-device matrix in #266.
No simulator camera or standalone typecheck can close that matrix. Manual broad QA
is reserved for the user's next test session.


## Native packaging evidence

The first local pod install exposed an empty aggregate target from a local `:path`
WebRTC spec; it was not accepted as engine installation. The corrected Expo Podfile
plugin uses standard `:podspec` resolution and installed the real checksum-pinned
`Pods/ZoenWebRTC/WebRTC.xcframework`. The lock diff added only ZoenWebRTC and the
changed local ZoenCore spec; all existing dependency versions stayed fixed. The
plugin also discloses microphone/camera call usage and preserves audio/background
notification modes. It does not advertise VoIP wake before PushKit is implemented.
