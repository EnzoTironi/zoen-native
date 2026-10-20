DeckCraft managed runtime for Zoen

Upstream: https://github.com/storytold/deckcraft
Revision: a31b990d32f2c8562f1588562f922960c3fde48b (0.9.0)
Upstream license: MIT OR Apache-2.0. Original licenses, NOTICE and ATTRIBUTION
are preserved in licenses/deckcraft/. Zoen's repository license is unchanged.

Local changes are the three exact pinned patches in patches/: an artifact-scoped
host bridge over the existing Session, followed by explicit managed startup and owner-queue freeze at durable close.
The original standalone application is neither embedded nor auto-started.
Source, lockfile and patch identities are recorded in upstream.json.

The managed build embeds the three exact craft-fonts inputs recorded in
font-input/upstream.json, each under OFL-1.1. Their attribution/licenses are
preserved in licenses/craft-fonts/. The upstream egui default fallback fonts
have separate notices in licenses/epaint_default_fonts/; the three selected
craft fonts do not describe every glyph resource in the binary.

The bridge adds RustCrypto SHA-256 dependencies; their exact license files are
preserved in the corresponding versioned license directories. The trusted JS
loader uses the application's already installed @noble/hashes 2.4.0 (MIT),
whose license is preserved in licenses/noble-hashes-2.4.0/.

These notices cover the local source changes and identified embedded fonts.
They do not claim an exhaustive audit of the complete locked Rust dependency
graph. A distributable release must retain applicable upstream third-party
notices and its own exact generated package manifest.

No upstream icon, favicon or brand asset is used as a Zoen icon. Trunk's
standalone index.html is a compiler output only, not the MCP App resource.
