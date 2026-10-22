[Native surface index](../expo-native-surfaces.md).

## Offline voice and media — dedicated signed v82a-offline

This separate fault artifact uses source `c71383a7` / 832 inputs / the same source
hash as v82, with `http://127.0.0.1:18782` compiled into distinct signed executable,
JavaScript and app-tree hashes. Only fresh synthetic iPhone DCD25F2B receives it;
main/human accepted v82 artifacts remain unchanged. Public native linking adds the
third device to two identities and displays the received history seed. Pairing
codes are absent from retained QA evidence and its clipboard is cleared.

The owned listener and existing HTTP/WS sockets are cut/drained to zero; the fixture
HTTP server has already stopped. Native Send occurs once for voice and once for
ordered photo/video with separate “QA v82a offline photo” / “QA v82a offline video”
captions. Actual offline cold retains all three queued IDs with no draft, while the
independent recipient still has zero. Automatic reconnect requires no resend.
The recipient counts **0 / 0 / 3 / 3** at pre/offline/post/online-cold checks, with
stable three IDs/items/v1 and sequence **14 < 16 < 18**; only the seed plus three
messages exist. One explicit recipient Read then appears on all three native rows.
Both cold returns show one native row per ID; final composer is empty, keyboard
closed and timeline 100%. The 24.060-second first-Delivered gate is not a benchmark.

Authenticated received digests match canonical manifests post/cold. Photo **8,530 B**
and video **3,306 B** equal their original fixture hashes. Rendered voice **116,346 B**
has a pinned digest and 160-bar waveform hash; **3,725 ms** is signed metadata.
No original voice-byte identity, independent full AAC/video decode, physical-duration
or cryptographic author-device certification is claimed; native origin is bounded
fresh-namespace/public-journey inference.

The 41 observations / 35 copied PNGs and retired 3/61-step recordings keep corrected
waits, pre-Send autocorrect/cursor corrections and overlay warnings. Early warm-keyboard
overlap at 85% followed by settled 100% is an unmeasured diagnostic, not a performance
claim. No formal replay/E2E. Helpers exit 0 with volatile state discarded; the 0600
consumed latch remains, proxy sockets/control retire, and only the fresh device's
servers stop before shutdown with its account data preserved. Shared relay PID 19737
and the existing v82 targets are untouched.

App 669/669, focused voice 86 and exact-source Expo CI keep v82 attribution; unchanged
Rust 463/463 keeps `6c338bcf`, with no new product code/tests. Broader outages/faults,
PDF Markup recovery, microphone/ASR, physical/provider/Push/accessibility/performance
and full parity remain open. Evidence: `/private/tmp/zoen-offline-media-qa-v82/native/runtime.json`,
`peer-checkpoints.json`, `accepted-artifact-verification.json` and cleanup receipts
in that root. This supersedes only the bounded offline-media preparation scope;
all earlier evidence retains its exact artifact attribution.
