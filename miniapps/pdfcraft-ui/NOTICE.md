# PdfCraft integration provenance

PdfCraft source is fixed to `d445410807e7eefbe5b96f9662ab963ae8dd2a3a`
from <https://github.com/storytold/pdfcraft>. Source is MIT OR Apache-2.0.
Retain upstream `LICENSE-MIT`, `LICENSE-APACHE`, `NOTICE` and `ATTRIBUTION.md`
with a distributed engine. The Zoen adapter is AGPL-3.0-only.

The real editor and parser remain upstream implementations. The source pin also
fixes its existing renderer/platform patches; preparation preserves them.
Shared eframe/egui input patches use the same archive verification and notices
as the qualified managed editor foundation.

The source's `docs/brand/LICENSE-brand.txt` explicitly requires forks and
distributed modified versions to remove ArtCraft trademark graphics. Those
graphics must not enter the managed package. Plain source attribution remains.

The native inspection creates its own original, two-page review text using the
upstream PDF writer. It uses no third-party documents or media. The opaque
catalog value is contributor-authored conformance data. Its fixed clock is
synthetic test input, never a production timestamp.

The initial inspection disables operating-system font discovery with
`PDFCRAFT_SYSTEM_FONTS=0`. Standard PDF fonts are rendered through the upstream
substitute fonts and their notices. Optional UI fonts, Lucide icons, CMaps and
ICC profiles retain their individual upstream licenses. Source availability is
not a license grant for trademarks, optional fonts or external user documents.

Preparation, compilation, native editor execution, browser execution, WASIp2,
canonical persistence and native iPhone acceptance are separate statuses. A
prepared source or screenshot does not make this App available in Zoen.
