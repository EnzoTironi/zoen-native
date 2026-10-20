# Telegram iOS reference catalog — imported snapshot

Telegram iOS is the primary messaging UI and interaction reference. Start with
the [product guide](../../docs/product/telegram-ios-reference.md), the
[searchable catalog](index.html), or the [five-surface study map](../../docs/product/telegram-ios-next-ui-study.md).
The catalog is research, not an implementation selection or a parity claim.

## Portable views

- [Readable matrix](matrix.md), [feature CSV](matrix.csv), [full JSON](matrix.json).
- [Premium / Business register](premium-business-register.csv).
- [Limits and source entrypoints](limits-and-entrypoints.csv).
- [Historical announcement register](announcement-register.csv).

All views and registers above, and the product guide, retain the exact bytes of
the user-provided snapshot. The HTML embeds its data and needs no localhost
server to display the catalog. Following official source links still needs a
network connection. No Telegram client code or assets are imported.

## Scope and provenance

The snapshot is dated 20 October 2026: schema 1, 506 unique capability rows,
23 areas, Telegram iOS commit `f1dd7a2dbd02cbbf513e75d5695d8d36d1cf5838`,
secondary Android commit `0aae959806c57be8ad457980f2a5c6277ff18635`, public API
layer 225. Frontend paths are **area-level entrypoints**. Every row explicitly
marks feature behavior as untraced and product runtime as untested. File existence
recorded by the original research is not evidence of per-feature implementation.
Premium, Business, boost, transaction and platform gates remain separate.

[Import provenance](import-provenance.json) records source/destination hashes,
counts, representation checks and the limits of this import. The
[original README](provenance/original-README.txt) is preserved as plain-text
provenance; its older reproduction instructions and sibling-research links are
not the navigation for this imported package.

`source-receipts.json`, `link-adjustments.json`, `validation.json` and
`viewer-check.json` retain the original research's receipts. The latter two are
historical artifact/viewer results, not tests rerun during this import and not
Telegram or Zoen runtime acceptance. The 164 captured official source pages were
hash-checked read-only at the supplied source location, but are not vendored here.
The older research package, its Android fork proposals, build/validation scripts
and captured repository trees are also excluded. Rebuilding or independently
repeating the original complete source validation requires that original package;
this portable catalog does not claim to contain a full reproduction environment.

Signal remains the reliability and behavioral-test reference; Notion and ChatGPT
Spaces remain the live-document and creation references. Zoen's sky, App cards,
full-screen Apps, agents and native/backend authority contracts remain intact.
No additional feature or acceptance journey is selected by this import.
