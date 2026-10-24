# Validate an exported Grid XLSX

`xlsx_export.py` is the existing browser fixture's file reader, extracted and
bounded for reuse with a file actually saved from the iPhone share sheet. It
uses Python's standard library and never extracts ZIP members to disk.

Run its regressions:

```sh
python3 -m unittest discover -s miniapps/gridcraft-ui/test -p test_xlsx_export.py -v
```

The two small XLSX fixtures are unchanged exports of the real pinned Grid
engine, with hashes and retained browser-receipt provenance in
`fixtures/exports.json`. They prove the reader can inspect those exports, not an
iPhone journey. Adversarial tests mutate copies only.

For the future iPhone journey, the operator must first capture the actual UI
export, selected Item/version and canonical receipt. Do not substitute one of
these fixtures for that file. This helper does not drive the app or establish
that close/reopen, approval, recovery or sharing occurred.

Write a numeric expectation file using the values required at that point in the
journey. For the planned manual edit, `expected-cells.json` contains:

```json
{"A1":{"value":"11"},"B2":{"value":"22"}}
```

Run against the actual saved file (absolute path in `EXPORTED_XLSX`):

```sh
python3 miniapps/gridcraft-ui/test/xlsx_export.py verify "$EXPORTED_XLSX" \
  --sheet Sheet1 --expect expected-cells.json
```

For a declared formula, include both its raw XLSX formula (without the UI's
leading `=`) and cached numeric result, such as
`{"A2":{"value":"14","formula":"A1*2"}}`. An omitted `formula` requires a
literal numeric cell. The helper compares the cached result; it does not
recalculate formulas. It refuses shared-string/boolean/error cells as numeric
proof. Do not assume that an approved edit leaves A1/B2 unchanged: use the exact
reviewed edit's expected cells and formula, then validate the separately saved
post-approval export and its cold-reopened export.

When available, pass `--metadata "$CAPTURED_EXPORT_METADATA"` with the actual
`prepare_document_export` structured metadata: `format`, `sourceHead` (string
`version` and `sha256`), export `sha256`, `bytes`, and optional `byteLength`.
The helper checks the SHA256 and length of the exact file bytes against that
record. Do not reconstruct an absent record from the file and call it a match
against an observed export.

Optionally pass `--context "$OPERATOR_CONTEXT"`, a small JSON projection from
the actual handoff/UI/canonical receipt with **only** `itemId` and `sourceHead`.
The latter has the same `version`/`sha256` shape. This checks consistency with the
captured metadata, and requires `--metadata`; it does not verify authenticity of
either supplied JSON file. No Item identity is embedded in the workbook by this
helper. The source document hash may differ from the exported file hash.

Output always includes `identityBindingVerified: false` and
`uiJourneyVerified: false`. `operatorContext` is explicitly supplied context,
never cryptographic proof that XLSX bytes belong to that Item/version. Preserve
this result beside the actual file, UI capture and canonical receipt for the
operator's journey report. Compare the separately captured heads/receipts at the
manual, reopened and approved phases; one successful file check cannot prove
continuity between them. No second version counter or persistence authority is
introduced.

Limits match the managed reader's useful QA scope: 8 MiB archive, 128 members,
4 MiB per member, 16 MiB total declared inflated bytes, 16 workbook sheets and
40,000 stored cells in the selected sheet, within 10,000 rows/256 columns. ZIP
metadata is checked for every member before any inflation. Actual reads are
bounded and CRC-checked; duplicate/unsafe member paths, symlinks, encryption,
unsupported compression, XML DTD/entities, ambiguous cells and selected
external workbook relationships refuse. Sheet selection follows the workbook's
name/relationship, rather than assuming `sheet1.xml`. The helper is a limited
file inspector, not a replacement for production profile validation, an Excel
compatibility validator, a macro scanner or a formula engine.

`cells` mode preserves the browser fixture's original raw value/formula JSON
shape. It is an inspection API; unlike `verify`, it does not assert numeric cell
types or expected contents. Malformed input exits nonzero without echoing XML,
row data, metadata, Item IDs or secrets to stderr.
