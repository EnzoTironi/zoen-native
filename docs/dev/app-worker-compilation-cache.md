# App worker compilation cache

The host can reuse compiled Wasmtime49 artifacts across fresh Engines and process
restarts. No document, account, input, output, grant or CAS receipt is cached here.
Existing `WasmTier::new` and `DeckcraftWorker::load_for` remain uncached.

To opt in, the operator creates an empty directory under their own protected
deployment tree, sets mode0700, and supplies its absolute normalized path through
`ZOEN_APP_WORKER_CACHE_DIRECTORY`. The ordinary `agent-host` worker loader shares
this optional cache among configured Deck/Grid/Word workers. It never reads a
global Wasmtime config. An invalid explicit cache configuration fails startup;
it is not silently ignored.

The directory must already exist, belong to the process user and have mode0700.
Symlinks and unsafe ancestor replacement permissions are refused. Root-owned
sticky system temporary ancestors are allowed; the cache itself remains private.
Ownership/permissions and original directory identity are rechecked before each
verified component load. Unix validation is required; other platforms explicitly
refuse this optional configuration. Operators must also keep ACLs and other
same-user processes within the trusted host boundary.

Publisher signature/trust, WASM tier and exact component SHA256 are verified
before `Component::new` performs its official cache lookup. Cache use does not
grant authority or remove canonical actor/review/CAS checks. The App executor
still has one concurrent run,5seconds,2billion fuel units,256MiB and its existing
bounded stdin/stdout, with no preopened files, inherited environment or network.

Compiled cache files contain native code. They must never be imported from an
App, model, user upload or an untrusted shared cache. A valid source SHA does not
authenticate an adulterated compiled artifact. Official Wasmtime handles a small
unreadable/truncated artifact as a miss and recompiles verified WASM; this is not
a guarantee for malicious native artifacts or decompression bombs. The configured
32entry/512MiB limits are **soft** asynchronous cleanup targets, not hard quotas.
Compilation also precedes the existing per-invocation5second execution deadline.

Focused tests:

```sh
cargo test -p zoen-agentd --lib wasm::cache::tests
cargo clippy -p zoen-agentd --all-targets -- -D warnings
```

The opt-in `app_worker_cache_check` example accepts the genuine Word WASIp2
component, a real accepted plain DOCX seed, an existing empty private cache and a
fresh receipt path. It measures a cold miss, a fresh-Engine hit and a new-process
hit; executes real `text.replace` and exact reopen; and checks reduced caller
fuel/memory are still refused. It does not create an account, contact a relay or
provider, or establish canonical/native acceptance.

```sh
cargo build -p zoen-agentd --example app_worker_cache_check
./target/debug/examples/app_worker_cache_check measure \
  /absolute/wordcraft-zoen-host.wasm /absolute/seed.docx \
  /absolute/operator-private-cache /absolute/fresh-result.json
```

Track measured source, component, receipt and binary hashes with #562; #176/#180
remain broader product/reliability acceptance.
