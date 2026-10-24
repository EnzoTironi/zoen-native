# Grid managed editor inputs

Upstream engine and UI: GridCraft 0.11.1, commit
`b8771d51fb4da60bd48c13cf313d6d6014dc5e66`, MIT OR Apache-2.0.
Exact notices are in `licenses/gridcraft/`. The ArtCraft name/wordmark/logos
under upstream `docs/brand/` are excluded. The managed shell does not load the
standalone page, account/storage/collaboration services or external images.

The real engine pulls its model, formula/functions/calculation, XLSX and its
small PDF writer; only XLSX is exposed. The UI uses `craft-ui` from `craft-libs`
commit `d27646a5f7c8b4fc85d9dd3213b8c88b58e0088d` and egui/eframe/egui_glow 0.36.2 over glow 0.17.0.
This is the existing upstream dependency graph, not adoption of the unrelated
Craft libraries. The exact compiled WASM graph and notices must accompany each
packaged output; native-only/build dependencies must not be called runtime assets.

The shell enables eframe's existing default fonts. On WASM the pinned UI has no
system-font discovery. The four embedded font files and their byte hashes are in
`font-input.json`: Ubuntu Light (Ubuntu Font Licence), Hack (its bundled Bitstream
Vera/DejaVu/Hack notices), Noto Emoji (SIL OFL 1.1), and emoji-icon-font (MIT).
The exact font notices are in `licenses/epaint-default-fonts/`. `craft-fonts` and
additional downloaded fonts/models are not used. No font name implies a license
for unrelated brand assets.

Zoen's authored host/shell is AGPL-3.0-only. Compilation and browser/iPhone
qualification are separate evidence; this inventory does not enable the App.
