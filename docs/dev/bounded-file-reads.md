# Bounded file reads and page import completion

Source checkpoint: 12 October 2026, shared `main`. The nine new file cases passed on the captured working source. Both relay journeys compiled but have not run yet; matching native bindings have not been regenerated. Earlier Core and app results belong to their recorded source snapshots.

## File API

| Rust API | Native binding | Behavior |
| --- | --- | --- |
| `file_bytes_bounded(item_id, version, max_bytes)` | `fileBytesBounded(itemId, version, maxBytes)` | Returns verified bytes of the selected version, within the caller's limit and the Core maximum. |
| `file_info_at(item_id, version)` | `fileInfoAt(itemId, version)` | Returns `FileDto` derived from exactly that version without restoring it. |
| `file_bytes(item_id, version)` | `fileBytes(itemId, version)` | Uses the same guarded reader with the existing 512 MiB Core maximum. |
| `file_thumbnail(item_id)` | `fileThumbnail(itemId)` | Returns the latest version's verified thumbnail within the 2 MiB limit. |

`version` is `Option<u32>`; `None` selects the latest version. `max_bytes` is `u64` and measures plaintext bytes. The bounded byte API returns `Result<Option<Vec<u8>>, CoreError>`; missing chunks return `None`, while invalid metadata, oversized cache rows and digest/length mismatches return an error. Corruption previously appeared as pending data through `fileBytes`; it now returns an error through both byte readers. Missing items or versions are errors.

Limits are checked before allocating the assembled file or returning a SQLite BLOB:

- Nonempty files, at most 512 MiB and at most the caller's limit.
- At most 513 chunk references, each nonempty and at most 16 MiB; their checked total must equal the declared file size.
- At most 128 KiB of file metadata text and fixed chunk-reference content; every plaintext digest, ciphertext address and key must be 64 hexadecimal characters.
- Nonempty thumbnails, at most 2 MiB.
- Every cached piece must match its declared size and plaintext digest. The assembled bytes must match the selected version's file digest.

The store checks `length(bytes)` before returning a payload with a conditional SQL selection in the same snapshot. The file reader checks all cached lengths before reserving output capacity and repeats bounded selection while reading each piece. Missing or oversized chunks therefore cannot trigger a file-sized allocation. Reads do not change signed history or the outbox.

Clients should pass the maximum their viewer can handle, using `fileInfoAt` for the same version they request. `FileDto.ready` reports whether all cached entries have the declared lengths; the byte reader still verifies their contents. A matching `ready` flag does not bypass the bounded read. The compatibility maximum is not a suitable memory budget for every mobile viewer.

## Page import completion

Online `zoen page import` waits for `wait_until_settled` using the command's existing timeout before printing imported IDs and paths. An unfinished sync returns a nonzero status. Imported pages remain saved locally for retry. `--offline` still accepts local imports without requiring a relay acknowledgement.

Settled means an online, caught-up connection with no active event outbox, unknown Spaces, uploads, profile work or MLS work left. This is the existing Core sync completion contract; it does not establish production delivery or explain every possible relay refusal.

PR70's hosted `pages_and_files_travel_byte_for_byte` failure is preserved. A subsequent local run of the unchanged journey passed before this completion fix. That local result does not identify the hosted failure's cause or certify its resolution.

## Verification

The current local unit compiled the Core/store/protocol libraries and both CLI journeys with the lock enforced. All nine new file cases passed in a focused run, with 222 other Core cases explicitly filtered. The complete store library passed nine cases; the complete protocol library passed 28, including the four APNs input/payload cases. These are separate results from the complete 231-case Core run.

That complete Core run was interrupted by the source guard after `crates/protocol/src/lib.rs` changed during execution. Its frozen hash was `404bfe6f89c9622d092f4e425e14c94c73711ac0447b8bca78e14e8155c4b713`; the observed file lost the `pub mod push` entry. The interrupted run is not a complete pass. No PostgreSQL or FoundationDB test service had started. An earlier unit also preserved a lock-resolution prerequisite failure before compilation. Both negatives and the actual partial results are retained in the local `.dev/c4-bounds-cli-v1` and `v2` artifacts.

Complete current-source Core acceptance and both actual relay journeys remain required after restoring and freezing the module entry. Existing external dependency versions were preserved by the offline lock resolution. No original test assertion, deadline or stack setting was relaxed.

`crates/client-core/src/tests/file_bounds.rs` adds nine behavioral cases for exact latest/historical limits and metadata, authenticated extreme sizes, invalid chunk references, oversized SQLite rows, missing chunks without mutations, cache/digest corruption, ordered assembly and thumbnail bounds/absence. Invalid metadata fixtures are signed file versions, rather than bypassing the log with a projection edit.

`crates/zoen-cli/tests/journey_page_completion.rs` uses two real accounts and an owned relay. It limits publication to one event per hour, imports two pages, requires a failure without completion IDs, checks their offline presence, removes the owned relay limit and requires identical exports on the other device. The original files journey and its deadlines/assertions remain unchanged.

After the coordinator freezes the Core inputs and reserves the build slot:

```sh
cargo test --locked -j2 -p client-core --lib -- --test-threads=1
cargo test --locked -j2 -p zoen-cli --test journey_page_completion -- --nocapture --test-threads=1
cargo test --locked -j2 -p zoen-cli --test journey_files -- --nocapture --test-threads=1
```

The relay journeys need owned PostgreSQL and FoundationDB services. Local runs retain the agreed resource limits and default stack. Source, compilation, complete Core tests, generated ABI, native viewers and hosted CI results must be recorded separately. No live provider or public relay deployment is part of this unit.
