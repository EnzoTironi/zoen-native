These bytes were produced by the real PhotoCraft document writer and managed
Session bridge at upstream `9b9643764f3fb77d9f0e48d7510983ae50621e11`.
No source images, fonts, trademarks or external assets were imported.

`native-asymmetric.pcraft` is a 640 × 480 RGB8 canvas with white background,
a black rectangle at (100, 90), size 60 × 30, and a green rectangle at
(420, 310), size 25 × 40. The pinned writer encoded the document unchanged.
SHA-256: `81588c99ceddd3b79f6038d310b4829cc727cc3c6edb6d677b51db7eeb11d028`.

`native-rotated.pcraft` is the actual `rotate_clockwise` proposal returned by
the managed bridge, invoking the upstream command through its real Session.
SHA-256: `351c27a4eee8474d12982331bc74f8dc62871425eb1ef8ee1afe27505bb1da30`.

These fixtures prove structural parser compatibility with actual native
producer output. They do not independently prove pixel fidelity, WASI execution,
browser rendering, canonical authorization or durable storage.
