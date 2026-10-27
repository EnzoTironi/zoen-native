# Source and distribution boundaries

PhotoCraft source at `9b9643764f3fb77d9f0e48d7510983ae50621e11` declares
MIT OR Apache-2.0. Original license texts are retained in `licenses/`. Zoen's
adapter is AGPL-3.0-only. The ArtCraft name, wordmark and logos are excluded from
the source license; do not ship the upstream brand as a Zoen asset.

The upstream NOTICE separately lists Inter/JetBrains Mono fonts (OFL), Lucide
icons (ISC/MIT), the SCOWL word list and libfacedetection weights/code (BSD-3).
The text crate embeds bundled fonts even in a headless dependency graph; the
algorithm crate contains face detection weights. A distributable package needs
the notices for its actual resolved/embedded dependencies, not just the two code
licenses copied here. No compiled PhotoCraft binary/assets are distributed by
this source-only foundation, and no complete dependency-license audit is claimed.

Do not fetch craft-fonts or providers automatically. The upstream build's
`CRAFT_FONTS_DIR` is optional natively and the standalone web app loads additional
fonts separately; those loaders require a managed, verified capability contract
before integration. External accounts, telemetry, downloads, print services,
plugins and collaboration servers are outside this adapter's command profile.
