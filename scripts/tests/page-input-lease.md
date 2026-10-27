# Page input lease on macOS

`scripts/test-page-input-lease.py` compiles the real generated Swift bindings,
`ZoenPageInputLease.swift` and `PageInputLeaseTests.swift` against a normally built
host `libclient_core.dylib`. It never builds Cargo or substitutes Core/storage.

Wait for the owner of the shared Cargo target to seal the **same-source** host
library, generated Swift and C FFI header/modulemap. Compare their recorded hashes
with the seal before invoking the runner. Use the installed Enriched Markdown
`ios/input/ENRMDraftSnapshot.h` from that app's pinned/patched dependency. The runner
records actual input hashes and rechecks them on completion; it does not generate
or independently certify the supplied Rust artifacts' build provenance.

```sh
python3 scripts/test-page-input-lease.py \
  --source-root /absolute/clean/source \
  --source-sha FULL_COMMIT_SHA \
  --client-swift /absolute/sealed/ClientFFI.generated.swift \
  --ffi-header /absolute/sealed/client_coreFFI.h \
  --ffi-modulemap /absolute/sealed/client_coreFFI.modulemap \
  --host-library /absolute/sealed/libclient_core.dylib \
  --checkpoint-header /absolute/app/node_modules/react-native-enriched-markdown/ios/input/ENRMDraftSnapshot.h \
  --output /absolute/new/external/qualification-directory
```

The output directory must not exist. Source lease/test files must exactly match
the named checkout commit. All compilation inputs are copied verbatim except the
dylib, whose original install name is preserved; supplied files are never edited,
rebound or signed. The C modulemap exposes the real Foundation-only header as
`ReactNativeEnrichedMarkdown`. No protocol declarations, producer implementation,
registry implementation, DTOs or fake Core are added. Swift caches stay in output.

The genuine test opens only its own fresh temporary account/SQLite files and
removes them on normal exit. It checks encrypted retained text/selection/IME
bytes, private Apply and exact replay, invalidation, malformed IME rejection,
durable Discard and whole-document replacement. It does not connect to a relay.
An interrupted/crashed test can leave its synthetic temporary directory for
diagnosis; this runner never removes unrelated directories.

`result.json` retains source, hashes, commands, exit codes, deadlines and logs.
Success requires compile/link success, test exit zero, its exact completion
marker, and unchanged explicit inputs. A failure remains a failure; the runner
does not retry. The default deadlines are 180 seconds compile and 60 seconds test.

This proves **macOS Foundation + actual Core journal/FFI behavior**, not UIKit,
the ENRM producer/registry implementation, IME/focus on iOS, or device acceptance.
