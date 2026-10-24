# WordCraft managed adapter inputs

Upstream WordCraft 0.9.0 at
`d1b0bfc3f9dd83126b162a42b667facecb003b7f` declares MIT OR Apache-2.0.
The unchanged source licenses and NOTICE are in `licenses/`. Zoen-authored
adapter sources are AGPL-3.0-only. The ArtCraft name, wordmark and logos are
excluded from the source license; no upstream brand assets are adopted here.

The headless adapter directly depends only on the document model, DOCX and
Session engine. The existing engine transitively includes CPU rendering/layout,
proofing and PDF writing; this is not a new GPU/UI dependency or a PDF export
claim. WordCraft pins craft-pdf and Krilla to craft-libs
`c3487320b2d1de657b860b3c8abb44e1f2b32bd8`, with MIT OR Apache-2.0 licensing.
The actual resolved graph and runtime assets must be recorded with each compiled
package; the presence of a transitive crate is not an enabled host capability.

The source engine embeds Source Sans 3, Source Serif 4, Inter and JetBrains Mono
fonts (SIL OFL 1.1); their upstream notices are included. Hyphenation material is
covered by the unchanged upstream NOTICE (public domain/CC0, MIT and BSD-3-Clause
as listed there). This recipe does not set CRAFT_FONTS_DIR, download craft-fonts,
or include official release icons/marks. Optional fonts, UI assets and all future
WASM package inputs require their own actual inventory before distribution.

Primary references: [pinned source](https://github.com/storytold/wordcraft/tree/d1b0bfc3f9dd83126b162a42b667facecb003b7f),
[upstream NOTICE](https://github.com/storytold/wordcraft/blob/d1b0bfc3f9dd83126b162a42b667facecb003b7f/NOTICE).
