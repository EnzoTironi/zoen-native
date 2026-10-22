# Canonical acceptance record

[`../acceptance.json`](../acceptance.json) remains the one ordered entrypoint for
the complete logical record. Its explicit `$part` references select maintained
JSON under `parts/`; these files are authored source, not generated output.
There is no committed expanded copy or second acceptance store.

From the repository root:

```sh
python3 scripts/acceptance.py check
python3 scripts/acceptance.py read
python3 scripts/acceptance.py read | jq '.currentIphoneArtifact'
python3 scripts/test-acceptance.py
```

Python callers use `load_acceptance` in `scripts/acceptance.py`; consumers must not
treat a raw `json.load` of the entrypoint as the expanded document. The loader
preserves strings and array/object order. It refuses absent, duplicate, cyclic,
unreferenced or escaping parts, symlinks, duplicate JSON keys and malformed JSON.
References have exactly one `$part` key, use relative POSIX `.json` paths, and
resolve from `acceptance/parts`, including references inside other parts.

The responsibilities follow the original record:

- `scope/`: product execution, Signal learning, creation requirements and gates.
- `artifacts/`: current, human and installed iPhone artifact records.
- `evidence/`: the original individual evidence packets and their ordered index.
- `history/`: retired backend, historical Git source and execution decisions.
- `checkpoints/`: current source/backend/activity checkpoints, post-merge packets,
  iOS continuity and DeckCraft's individual integration/runtime batches.

All thirty journeys, six platforms, ten creation capabilities, ten known gaps,
the three incomplete top-level acceptance flags and historical evidence remain
part of the composed record. Existing historical Git URLs, read commands, receipt
paths and qualifications retain their exact original strings. References to those
historical immutable manifests do not redirect to the current composition.

Edit the appropriate canonical part directly, then run `check` and the tests.
For a controlled replacement of one existing referenced part:

```sh
python3 scripts/acceptance.py update-part scope/creation-workspace.json \
  --from-json /absolute/path/reviewed-part.json
```

The writer validates the entire proposed graph before replacing the source file
atomically and preserves its mode. Failed validation/replacement leaves existing
source intact. It does not claim concurrent-writer CAS, a multi-file transaction,
product-data persistence or new acceptance. Restructuring the graph is a normal
reviewed source edit followed by validation. Keep every authored file at most
20,000 bytes; separate cohesive packets rather than arbitrary text chunks.
