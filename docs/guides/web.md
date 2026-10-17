# How to run the web client

**Goal:** run Zoen in a browser. There are two entry points in `packages/zoen-ui`; pick the one you need. **Requires:** Node.js 22+ and npm.

## Shell preview (sample data)

The reusable React application shell with static, labelled sample conversations — for layout, theme and interaction review. It sends nothing.

```sh
npm ci --prefix packages/zoen-ui --ignore-scripts
npm run typecheck:shell --prefix packages/zoen-ui
npm run test:shell --prefix packages/zoen-ui
npm run preview --prefix packages/zoen-ui
```

Open `http://127.0.0.1:4173` (loopback only).

## Authenticated browser client

The real client: paired by your phone, protocol 4, OpenMLS in a worker, a non-exportable WebCrypto signing key and an encrypted IndexedDB session. **Also requires** the `wasm32-unknown-unknown` Rust target and wasm-bindgen CLI 0.2.129.

```sh
rustup target add wasm32-unknown-unknown
cd packages/zoen-ui
npm ci
npm run typecheck:web
npm run test:web
ZOEN_WASM_BINDGEN=/absolute/path/to/wasm-bindgen npm run build:web
npm run web
```

Open `http://127.0.0.1:4174` (`ZOEN_WEB_PORT` overrides the port). Supply the relay in the pairing form or with `?relay=http://127.0.0.1:8787`; remote relays require HTTPS.

Shared-core checks from the repository root:

```sh
cargo test -p roda-mls --features portable
cargo test -p zoen-web-core
cargo check -p zoen-web-core --target wasm32-unknown-unknown
```

> **Status:** real native/browser pairing, MLS interoperability and reload/reconnect journeys are still open. Details: [web client README](../../packages/zoen-ui/web/README.md).
