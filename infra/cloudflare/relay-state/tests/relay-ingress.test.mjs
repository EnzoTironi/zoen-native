import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";
import { Miniflare, convertV4MiniflareOptions } from "miniflare";

const root = fileURLToPath(new URL("../", import.meta.url));
const native = JSON.parse(
  await readFile(
    process.env.ZOEN_DIRECTORY_FIXTURE
      ? resolve(process.env.ZOEN_DIRECTORY_FIXTURE)
      : join(root, "generated/directory-fixture.json"),
    "utf8",
  ),
);
const login = native.steps.find((step) => step.op === "login" && step.expected.kind === "session");
assert.ok(login, "Require actual native codec/certified-device fixture");
const settings = {
  RELAY_PROTOCOL_ENABLED: "true",
  RELAY_ALLOWED_ORIGINS: '["https://app.tryzoen.com"]',
};
function worker(name, paths, extra) {
  return {
    name,
    modulesRoot: root,
    compatibilityDate: "2026-10-18",
    modules: paths.map((path) => ({ type: "ESModule", path: join(root, path) })),
    ...extra,
  };
}
function runtime({ edge = {}, directory = {}, entrypoint = "RelayIngress", bound = true } = {}) {
  const target = worker(
    "directory",
    [
      "tests/relay-ingress-fixture.mjs",
      "src/directory-index.mjs",
      "src/relay-entrypoint.mjs",
      "src/relay-route.mjs",
      "src/directory-state.mjs",
      "src/relay-composition.mjs", "src/relay-presence.mjs", "src/socket-transport.mjs", "src/socket-alarm.mjs",
      "src/provider-auth.mjs",
      "src/media-blobs.mjs",
      "src/blob-io.mjs",
      "src/blob-quota.mjs",
      "src/transfer-blobs.mjs",
      "generated/wasm/admission.js",
    ],
    {
      bindings: {
        ...settings,
        ...directory,
        DIRECTORY_CELL: "owned-ingress-qa",
        RELAY_NAME: "zoen-cloudflare-private",
      },
      durableObjects: { DIRECTORY: { className: "DirectoryState", useSQLite: true } },
      serviceBindings: { CLOSED_RELAY: "closed" },
    },
  );
  target.modules.push({
    type: "CompiledWasm",
    path: join(root, "generated/wasm/admission_bg.wasm"),
  });
  const closed = worker(
    "closed",
    [
      "src/closed-index.mjs",
      "src/closed-space.mjs",
      "src/space-storage.mjs",
      "src/relay-publications.mjs",
      "generated/wasm/admission.js",
    ],
    {
      durableObjects: { CLOSED_SPACE: { className: "ClosedSpace", useSQLite: true } },
      serviceBindings: { RELAY_DIRECTORY: { name: "directory", entrypoint: "RelayDispatch" } },
    },
  );
  closed.modules.push({
    type: "CompiledWasm",
    path: join(root, "generated/wasm/admission_bg.wasm"),
  });
  return new Miniflare(
    convertV4MiniflareOptions({
      workers: [
        worker("ingress", ["src/relay-index.mjs", "src/relay-route.mjs"], {
          bindings: { ...settings, ...edge },
          ...(bound
            ? { serviceBindings: { DIRECTORY_RELAY: { name: "directory", entrypoint } } }
            : {}),
        }),
        target,
        closed,
      ],
    }),
  );
}
async function inventory(mf) {
  const namespace = await mf.getDurableObjectNamespace("DIRECTORY", "directory");
  return JSON.parse(await namespace.getByName("owned-ingress-qa").ingressInventory());
}
function message(socket) {
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => {
      cleanup();
      reject(new Error("Missing actual binary handshake response"));
    }, 5000);
    const received = (event) => {
      cleanup();
      resolve(Buffer.from(event.data));
    };
    const closed = () => {
      cleanup();
      reject(new Error("Socket closed before handshake response"));
    };
    function cleanup() {
      clearTimeout(timer);
      socket.removeEventListener("message", received);
      socket.removeEventListener("close", closed);
    }
    socket.addEventListener("message", received);
    socket.addEventListener("close", closed);
  });
}

test("service-bound ingress rejects raw RPC paths, namespace selectors, methods and unapproved browser origins before creating a socket", async () => {
  const mf = runtime();
  try {
    for (const path of [
      "/",
      "/hello",
      "/authenticate",
      "/publish",
      "/append",
      "/inspect",
      "/relayDispatch",
      "/v1/auth/google",
      "/v1/sync/",
      "/v1/sync?cell=other",
      "/v1/sync?session=forged",
      "/v1/sync?method=prepareRelayPublication",
    ])
      assert.equal(
        (await mf.dispatchFetch(`https://relay.test${path}`, { headers: { Upgrade: "websocket" } }))
          .status,
        404,
        path,
      );
    assert.equal((await mf.dispatchFetch("https://relay.test/v1/sync")).status, 404);
    assert.equal(
      (
        await mf.dispatchFetch("https://relay.test/v1/sync", {
          method: "POST",
          body: "{}",
        })
      ).status,
      404,
    );
    for (const origin of [
      "null",
      "https://evil.test",
      "https://app.tryzoen.com.evil.test",
      "https://app.tryzoen.com/path",
      "https://app.tryzoen.com, https://evil.test",
    ])
      assert.equal(
        (
          await mf.dispatchFetch("https://relay.test/v1/sync", {
            headers: { Upgrade: "websocket", Origin: origin },
          })
        ).status,
        403,
        origin,
      );
    assert.deepEqual(await inventory(mf), { connections: 0, challenges: 0, sessions: 0 });
    const privateWorker = await mf.getWorker("directory");
    assert.equal(
      (
        await privateWorker.fetch("https://private.test/v1/sync", {
          headers: { Upgrade: "websocket" },
        })
      ).status,
      404,
    );
  } finally {
    await mf.dispose();
  }
});

test("disabled protocol, invalid origin configuration and missing service binding fail without establishing a session", async () => {
  for (const options of [
    { edge: { RELAY_PROTOCOL_ENABLED: "false" } },
    { directory: { RELAY_PROTOCOL_ENABLED: "false" } },
    { edge: { RELAY_ALLOWED_ORIGINS: '["*"]' } },
    { directory: { RELAY_ALLOWED_ORIGINS: "[]" } },
    { edge: { RELAY_ALLOWED_ORIGINS: '["http://app.tryzoen.com"]' } },
    { bound: false },
  ]) {
    const mf = runtime(options);
    try {
      const response = await mf.dispatchFetch("https://relay.test/v1/sync", {
        headers: { Upgrade: "websocket" },
      });
      assert.equal(response.status, 503, JSON.stringify(options));
      assert.equal(response.headers.get("Cache-Control"), "no-store");
      assert.deepEqual(await inventory(mf), { connections: 0, challenges: 0, sessions: 0 });
    } finally {
      await mf.dispose();
    }
  }
});

test("native and approved-browser upgrades reach the same real binary Hello/challenge path through the named service", async () => {
  const mf = runtime();
  const sockets = [];
  try {
    const challenges = [];
    for (const origin of [null, "https://app.tryzoen.com"]) {
      const response = await mf.dispatchFetch("https://relay.test/v1/sync", {
        headers: { Upgrade: "websocket", ...(origin ? { Origin: origin } : {}) },
      });
      assert.equal(response.status, 101);
      const socket = response.webSocket;
      assert.ok(socket);
      sockets.push(socket);
      socket.accept();
      const challenge = message(socket);
      socket.send(Buffer.from(login.hello_base64, "base64"));
      const bytes = await challenge;
      assert.ok(bytes.length > 0);
      challenges.push(bytes);
      const refused = message(socket);
      socket.send(Buffer.from(native.transport.ping_base64, "base64"));
      assert.deepEqual(
        await refused,
        Buffer.from(native.transport.malformed_auth_error_base64, "base64"),
        "Auth is still required; an Upgrade/Origin cannot authenticate a device",
      );
    }
    assert.notDeepEqual(
      challenges[0],
      challenges[1],
      "Production Directory creates a new challenge for each actual connection",
    );
    assert.equal((await inventory(mf)).sessions, 0);
  } finally {
    for (const socket of sockets)
      try {
        socket.close();
      } catch {}
    await mf.dispose();
  }
});

test("candidate configuration keeps routes disabled and fixes the named service destination", async () => {
  const config = JSON.parse(await readFile(join(root, "wrangler.relay.jsonc"), "utf8"));
  const directory = JSON.parse(await readFile(join(root, "wrangler.directory.jsonc"), "utf8"));
  assert.equal(config.workers_dev, false);
  assert.equal(config.preview_urls, false);
  assert.deepEqual(config.routes, []);
  assert.equal(config.vars.RELAY_PROTOCOL_ENABLED, "false");
  assert.equal(directory.vars.RELAY_PROTOCOL_ENABLED, "false");
  assert.deepEqual(config.services, [
    { binding: "DIRECTORY_RELAY", service: directory.name, entrypoint: "RelayIngress" },
  ]);
  const closed = JSON.parse(await readFile(join(root, "wrangler.closed.jsonc"), "utf8"));
  assert.equal(closed.workers_dev, false);
  assert.equal(closed.preview_urls, false);
  assert.deepEqual(closed.routes, []);
  assert.deepEqual(directory.services, [{ binding: "CLOSED_RELAY", service: closed.name }]);
  assert.deepEqual(closed.services, [
    { binding: "RELAY_DIRECTORY", service: directory.name, entrypoint: "RelayDispatch" },
  ]);
});
