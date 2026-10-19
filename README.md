# Zoen

Talk to people and agents in the same place. Keep the plans, pages, files and mini-apps produced by the conversation under your control.

© 2026 Enzo Tironi. All rights reserved. Public for viewing only. See [LICENSE](LICENSE).

The shared Rust core powers the existing native clients and the Expo application for iOS, Android and Web. Current validation covers those three Expo platforms; desktop replacement is deferred.

## Current progress

The [Expo acceptance manifest](apps/zoen/acceptance.json) records current artifacts,
required journeys and exact evidence. Full comparative product parity is incomplete.

| Platform           | Accepted boundary                                                                                                                                                                                                                            | Remaining scope                                                                                                                                          |
| ------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------- |
| iPhone             | Signed v85 whole-application baseline; later v89–v92 bounded Excalidraw/MCP and preserving Home checks.                                                                                                                                      | v94/v95 Page-input/session foundations remain source/host proof; integrated editor recovery and remaining product journeys need signed runtime evidence. |
| Web                | 22 release checkpoints pass in English and Portuguese locally and in CI, including actual Excalidraw canvas edits, exact native-peer bytes and cold reload.                                                                                  | Broader drawing/media failures, rich file replies/captions/threads, startup offline recovery and broad accessibility remain open.                        |
| Android            | Actual API35 Debug app passed linking, history recovery, Page creation/read, header/status checks and local erasure with retained peer history. Packaged JNI and 14 JVM tests passed; independent CI also produced the minified Release APK. | Release runtime/performance, API28/physical devices and remaining media/platform features need acceptance.                                               |
| Backend            | Native Rust with PostgreSQL/FDB/S3-compatible storage; the local production-like composition is being qualified.                                                                                                                             | Public provider login, complete production/restore/load evidence remain open.                                                                            |
| Static Web hosting | app.tryzoen.com has dated public asset/header/integrity evidence.                                                                                                                                                                            | A hosted frontend does not qualify the native backend.                                                                                                   |

See [Web acceptance](docs/dev/expo-web-acceptance.md),
[Android acceptance](docs/dev/expo-android-acceptance.md),
[iPhone native evidence](docs/dev/expo-native-surfaces.md) and
[static Web hosting](infra/cloudflare/README.md) for the boundaries and reproduction.
The custom Cloudflare backend port is retired; native development and operation use
the [owned local environment](infra/local/compose.yaml).
Builds and bounded checks do not establish complete replacement or billion-user capacity.

The earlier merged-client results retain their dated attribution in the
[12 October handoff](docs/dev/handoff-2026-10-12.md) and the
[immutable prior README](https://github.com/EnzoTironi/zoen-native/blob/e76c74be4af4bfe483f4de4b63b6f932a83f0d8b/README.md).

## Product navigation

Chats contains direct conversations, groups and communities. Optional filters narrow it; community permissions and resources stay inside the conversation. Notifications open as cards, with explicit switches to a list and back. Plans use the same card and Live Page presentation as other items. Desktop and web follow the iOS themes and conversation hierarchy.

## Development

```sh
rustup toolchain install 1.99.0 --profile minimal --component rustfmt,clippy
export RUSTUP_TOOLCHAIN=1.99.0
cargo fmt --all --check
scripts/build-core.sh
```

Apple builds require Xcode, XcodeGen and the Rust Apple targets. Run `scripts/dev-stack.sh` in a separate terminal; it remains attached to the relay. The full Rust suite also requires PostgreSQL, FoundationDB, NATS and the sandbox providers configured by [CI](.github/workflows/ci.yml). Read [development setup](docs/dev/setup.md) before account/device journeys. The [web package](packages/zoen-ui/README.md) describes the reusable shell and preview.

The certified runtime reads reply receipts in batches of at most 64 real history positions. A prefix retains every pending event, retry field, reference, and sealed publication byte for cold restore. Concurrent member messages can share an earlier verified `seen` without proving a fork. A signed `seen` that contradicts the held chain, or an authenticated checkpoint with an incompatible prefix or group digest, still parks sending and rejects cold activation. Restore also requires the exact certified agent device and owner bound to the image.

Run the receipt and fork regressions with:

```sh
cargo test --locked -j2 -p client-core --lib runtime_core::tests::receipt_prefix_keeps_exact_pending_bytes_and_resumes_beyond_64_real_positions -- --exact --test-threads=1
cargo test --locked -j2 -p client-core --lib runtime_core::tests -- --test-threads=1
cargo test --locked -j2 -p client-core --lib mls::tests -- --test-threads=1
```

### Markdown import performance

`journey_files` measures import through relay acceptance of every sealed page. It exports every page on another device and compares the original bytes. The default hosted budget is 250 ms per page. The F5 measurement corpus had 94 UTF-8 Markdown files under `docs`, with 1,137,578 bytes in total. F7 adds documentation to the imported corpus; the aggregate candidate needs fresh corpus counts, hashes, timing and round-trip evidence.

Debug import spends most of its time on one CPU in Loro documents, message cryptography, and durable-queue decoding. `Cargo.toml` sets `opt-level = 3` for the sampled dependencies `loro-internal`, `loro-kv-store`, `sha2`, `chacha20`, `poly1305`, `serde_json`, `memchr`, and `foldhash`. The four core libraries `documents`, `event-log`, `mls`, and `store` have the same override because generic code uses the profile of the crate that instantiates it. [Cargo profile overrides](https://doc.rust-lang.org/cargo/reference/profiles.html#overrides-and-generics) explain that boundary. Debug assertions and overflow checks retain the development defaults. The application publish window is five messages per Space, matching the MLS receiver's out-of-order tolerance.

Compare five runs of `cargo test --locked -j2 -p zoen-cli --test journey_files -- --test-threads=1 --nocapture` on each revision. Keep the Rust toolchain, debug settings, two build jobs, disabled incremental compilation, and private Postgres and FoundationDB instances identical. Record the median and range of `IMPORT`, the successful byte count from `ROUND TRIP`, and compile cost from a separate `--timings` run. Profile CPU separately from reported timings. Keep the corpus unchanged between sides. These notes live in the root README so documentation does not add a page to that corpus.

## Documentation

Start with the [documentation index](docs/README.md), [current product checkpoint](docs/dev/zoen-product-progress.md), [current roadmap](docs/roadmap-status.md), [architecture](docs/architecture.md) and [system design](docs/system-design.md). The [Live Page contract](docs/product/live-pages.md), [session API](docs/dev/live-page-session.md) and [HIG matrix](docs/product/hig-compliance.md) separate implemented behavior from required acceptance. The [source auditor](tools/hig-audit/README.md) is advisory. [Dated history](docs/history/roadmap-2026-10-12-before-consolidation.md) preserves older results.

The product and built-in agent are Zoen. Internal modules use purpose-based names such as `miniapps`, `protocol` and `client-core`; see the [naming policy](docs/dev/naming.md). Zoen has not launched: the [prelaunch policy](docs/dev/prelaunch-policy.md) requires direct caller/test updates and coherent disposable fixtures, without compatibility aliases or dual reads/writes. Native artifacts must be regenerated and verified after interface changes.
