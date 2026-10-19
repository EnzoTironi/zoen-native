# Naming

The product and built-in agent are **Zoen**. Internal modules use their purpose,
without a product prefix. Reserve product names for app identity, user-visible
branding, executable entry points and externally scoped integrations.

## Current source layout

The shared Rust packages are `types`, `localization`, `miniapps`, `device-link`,
`event-log`, `grants`, `store`, `client-core`, `documents`, `gate`, `protocol`,
`relay-admission` and `mls`. Their workspace dependencies, source imports, build
scripts, CI paths and living documentation must refer to these names directly.
`event-log` avoids a collision with the third-party logging crate; `client-core`
avoids Rust's standard `core` namespace.

The native client object is `Client`; its internal state machine remains
`engine::Engine`. UniFFI generates the `client_core` ABI. The Apple package is
`ClientCore`, with the `ClientFFI` binary target. Regenerate language bindings and
native libraries from the same source; renaming an old generated artifact does
not make it a valid new one. The Expo module's public registration remains
`ZoenCore`.

Built-in MCP App resources use `ui://builtin/`. These identifiers are part of
signed app content, so new fixtures must come from actual signing producers.
Never edit the bytes of previously signed events to make a test match.

Account and demo databases use `account/client.sqlite` and `demo/client.sqlite`
under Application Support on Apple and `noBackupFilesDir` on Android. The app's
container already provides its namespace. Preserve backup exclusion, file protection,
secure key storage and restartable erasure. Development data is disposable;
an existing installed test account must not be silently destroyed by a source
rename or a build command.

## Prelaunch changes and verification

Follow the [prelaunch policy](prelaunch-policy.md): update callers and tests
directly, without compatibility aliases or duplicate reads/writes. Existing
cryptographic domains already identify Zoen and retain their exact bytes.
Product renaming alone is no reason to alter authenticated content.

Verify workspace resolution, generated client contracts, each native artifact,
MCP resource access and real encrypted interoperability. Source changes and
package metadata do not establish a passed native runtime journey. Until fresh
artifacts pass, earlier evidence keeps its original source and limitations.

Historical receipts, source manifests, commit links and dated documents retain
their original attribution. They are evidence, not active compatibility paths.
