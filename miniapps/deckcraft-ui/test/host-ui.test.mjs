import test from "node:test";
import assert from "node:assert/strict";
import { hostLayout, installManagedHostUI } from "../src/host-ui.ts";
const tick = () => new Promise((resolve) => setImmediate(resolve));
const context = {
  containerDimensions: { width: 430, height: 932 },
  safeAreaInsets: { top: 62, right: 0, bottom: 34, left: 0 },
  zoen: { chrome: { closeButton: { top: 70, right: 12, width: 44, height: 44 } } },
};
test("official context becomes finite frame-local geometry, preserving exact native close corner", () => {
  assert.deepEqual(hostLayout(context), {
    width: 430,
    height: 932,
    insets: { top: 62, right: 0, bottom: 34, left: 0 },
    closeButton: { x: 374, y: 70, width: 44, height: 44 },
  });
  const left = {
    ...context,
    containerDimensions: { width: 932, height: 430 },
    safeAreaInsets: { top: 0, right: 0, bottom: 21, left: 62 },
    zoen: { chrome: { closeButton: { top: 8, right: 12, width: 44, height: 44 } } },
  };
  assert.equal(hostLayout(left).closeButton.x, 876);
  const right = {
    ...left,
    safeAreaInsets: { top: 0, right: 62, bottom: 21, left: 0 },
    zoen: { chrome: { closeButton: { top: 8, right: 74, width: 44, height: 44 } } },
  };
  assert.equal(hostLayout(right).closeButton.x, 814);
  for (const bad of [
    { ...context, containerDimensions: { width: NaN, height: 932 } },
    { ...context, safeAreaInsets: { top: 1000, right: 0, bottom: 0, left: 0 } },
    {
      ...context,
      zoen: { chrome: { closeButton: { top: 0, right: 500, width: 44, height: 44 } } },
    },
    { ...context, containerDimensions: { width: 20000, height: 932 } },
  ])
    assert.equal(hostLayout(bad), null);
});
test("layout owner coalesces rotations behind its actual pending request and drops late publication after retirement", async () => {
  let resolve;
  const held = new Promise((done) => {
    resolve = done;
  });
  const inputs = [];
  let callback;
  const errors = [];
  const ui = installManagedHostUI({
    itemId: "owned",
    ready: () => true,
    onError: (e) => errors.push(e),
    app: { callServerTool: async () => ({}) },
    engine: {
      zoenHostOnExport: (next) => {
        callback = next;
      },
      zoenHostRequest: async (json) => {
        inputs.push(JSON.parse(json));
        if (inputs.length === 1) await held;
        return { metadata: '{"layoutAccepted":true}', bytes: new Uint8Array() };
      },
    },
  });
  const first = ui.layout(context);
  void ui.layout({ ...context, containerDimensions: { width: 440, height: 932 } }).catch(() => {});
  void ui.layout({ ...context, containerDimensions: { width: 450, height: 932 } }).catch(() => {});
  assert.equal(inputs.length, 1);
  resolve();
  await first;
  await tick();
  assert.equal(inputs.length, 2);
  assert.equal(inputs[1].input.width, 450);
  assert.equal(inputs[0].op, "layout");
  ui.dispose();
  await assert.rejects(ui.layout(context), /retired/);
  await tick();
  assert.equal(inputs.length, 2);
  assert.equal(callback, undefined);
  assert.deepEqual(errors, []);
});
test("menu accepts only one live PDF/PPTX intent and never sends document bytes or a URL", async () => {
  let callback;
  let resolve;
  const held = new Promise((done) => {
    resolve = done;
  });
  let ready = false;
  const calls = [];
  const errors = [];
  const ui = installManagedHostUI({
    itemId: "owned-artifact",
    ready: () => ready,
    onError: (e) => errors.push(e),
    app: {
      callServerTool: async (input) => {
        calls.push(input);
        await held;
        return { metadata: '{"layoutAccepted":true}', bytes: new Uint8Array() };
      },
    },
    engine: {
      zoenHostOnExport: (next) => {
        callback = next;
      },
      zoenHostRequest: async () => ({}),
    },
  });
  const old = callback;
  assert.equal(old("pdf"), false);
  ready = true;
  assert.equal(old("png"), false);
  assert.equal(old("pdf"), true);
  assert.equal(old("pptx"), false);
  assert.deepEqual(calls, [
    { name: "app_binary_share", arguments: { id: "owned-artifact", format: "pdf" } },
  ]);
  ui.dispose();
  assert.equal(old("pdf"), false);
  resolve();
  await tick();
  assert.deepEqual(errors, []);
});
test("native export refusal preserves ability to make a fresh explicit user intent", async () => {
  let callback;
  let attempts = 0;
  const errors = [];
  const ui = installManagedHostUI({
    itemId: "owned",
    ready: () => true,
    onError: (e) => errors.push(e),
    app: { callServerTool: async () => ({ isError: ++attempts === 1 }) },
    engine: {
      zoenHostOnExport: (next) => {
        callback = next;
      },
      zoenHostRequest: async () => ({}),
    },
  });
  assert.equal(callback("pdf"), true);
  await tick();
  assert.equal(errors.length, 1);
  assert.equal(callback("pptx"), true);
  await tick();
  assert.equal(attempts, 2);
  ui.dispose();
});

test("startup diagnostics whitelist fixed nonpayload classes and refuse arbitrary exception text", async () => {
  const { startupFailureCode } = await import("../src/host-ui.ts");
  assert.equal(startupFailureCode('{"code":"graphics_unavailable"}'), "graphics_unavailable");
  assert.equal(startupFailureCode('{"code":"canvas_missing"}'), "canvas_missing");
  assert.equal(
    startupFailureCode(
      Object.assign(new Error("Private implementation detail"), {
        zoenStartupCode: "module_init_failed",
      }),
    ),
    "module_init_failed",
  );
  assert.equal(startupFailureCode('{"code":"runtime_start_failed"}'), "runtime_start_failed");
  for (const error of [
    "private document content",
    '{"code":"private-name"}',
    "x".repeat(4097),
    { code: "graphics_unavailable" },
  ])
    assert.equal(startupFailureCode(error), undefined);
});

test("host appearance uses the existing bounded UI-only layout and a theme-only update is acknowledged", async () => {
  const calls = [];
  const ui = installManagedHostUI({
    itemId: "owned",
    ready: () => true,
    onError: (error) => {
      throw error;
    },
    app: {
      callServerTool: async () => {
        throw Error("Appearance cannot mutate documents");
      },
    },
    engine: {
      zoenHostOnExport: () => {},
      zoenHostRequest: async (json, bytes) => {
        calls.push(JSON.parse(json));
        assert.equal(bytes.byteLength, 0);
        return { metadata: '{"layoutAccepted":true}', bytes: new Uint8Array() };
      },
    },
  });
  await ui.layout({ ...context, theme: "dark" });
  await ui.layout({ ...context, theme: "dark" });
  await ui.layout({ ...context, theme: "light" });
  assert.equal(calls.length, 2);
  assert.deepEqual(
    calls.map((call) => [call.op, call.input.theme]),
    [
      ["layout", "dark"],
      ["layout", "light"],
    ],
  );
  for (const theme of ["system", true, {}, null])
    assert.equal(hostLayout({ ...context, theme }), null);
  ui.dispose();
});

test("shared SDK adapter snapshots trusted formats while keeping unsupported guest intents refused", async () => {
  const { installManagedHostUI: installShared } =
    await import("../../../packages/zoen-miniapp-sdk/src/managed-host-ui.ts");
  const formats = ["pdf"];
  let callback;
  const calls = [];
  const ui = installShared({
    engine: {
      applyHostLayout: async () => ({
        metadata: '{"layoutAccepted":true}',
        bytes: new Uint8Array(),
      }),
      setExportIntentHandler: (next) => {
        callback = next;
      },
    },
    exportFormats: formats,
    share: async (format) => {
      calls.push(format);
      return {};
    },
    exportRefusedMessage: "Trusted adapter share refusal",
    ready: () => true,
    onError: (error) => {
      throw error;
    },
  });
  formats.push("pptx");
  formats.splice(0, 1);
  assert.equal(
    callback("pptx"),
    false,
    "later config mutation cannot broaden the installed policy",
  );
  assert.equal(callback("pdf"), true, "the installed trusted snapshot remains exact");
  await tick();
  assert.deepEqual(
    calls,
    ["pdf"],
    "only the format crosses this adapter, never guest artifact/actor fields",
  );
  assert.equal(callback('{"format":"pdf","actor":"guest"}'), false);
  ui.dispose();
});

test("shared pending-layout retirement ignores late ACK/errors and cannot admit export into a retired owner", async () => {
  const { installManagedHostUI: installShared } =
    await import("../../../packages/zoen-miniapp-sdk/src/managed-host-ui.ts");
  let release;
  const held = new Promise((resolve) => {
    release = resolve;
  });
  let callback;
  let requests = 0;
  const errors = [];
  const ui = installShared({
    engine: {
      applyHostLayout: async () => {
        requests++;
        await held;
        return undefined;
      },
      setExportIntentHandler: (next) => {
        callback = next;
      },
    },
    exportFormats: ["pdf"],
    share: async () => {
      assert.fail("retired UI must not request sharing");
    },
    exportRefusedMessage: "Trusted adapter share refusal",
    ready: () => true,
    onError: (error) => errors.push(error),
  });
  const old = callback;
  const first = ui.layout(context);
  const queued = ui.layout({ ...context, containerDimensions: { width: 440, height: 932 } });
  ui.dispose();
  assert.equal(old("pdf"), false);
  assert.equal(callback, undefined);
  release();
  await Promise.all([first, queued]);
  assert.equal(requests, 1);
  assert.deepEqual(errors, [], "a late malformed ACK cannot publish into a retired UI");
  await assert.rejects(ui.layout(context), /retired/);
});

test("shared layout adapter preserves the original bounded exact ACK contract", async () => {
  const { installManagedHostUI: installShared } =
    await import("../../../packages/zoen-miniapp-sdk/src/managed-host-ui.ts");
  for (const reply of [
    undefined,
    { metadata: {}, bytes: new Uint8Array() },
    { metadata: "x".repeat(1025), bytes: new Uint8Array() },
    { metadata: '{"layoutAccepted":false}', bytes: new Uint8Array() },
    { metadata: '{"layoutAccepted":true}', bytes: new Uint8Array([1]) },
  ]) {
    const errors = [];
    const ui = installShared({
      engine: {
        applyHostLayout: async () => reply,
        setExportIntentHandler: () => {},
      },
      exportFormats: ["pdf"],
      share: async () => ({}),
      exportRefusedMessage: "Trusted adapter share refusal",
      ready: () => true,
      onError: (error) => errors.push(error),
    });
    await assert.rejects(ui.layout(context), /acknowledge host geometry/);
    assert.equal(errors.length, 1);
    ui.dispose();
  }
});

test("Deck facade keeps its trusted item/tool/formats when layout metadata advertises another editor", async () => {
  let callback;
  const calls = [];
  const ui = installManagedHostUI({
    itemId: "trusted-deck-item",
    ready: () => true,
    onError: (error) => {
      throw error;
    },
    app: {
      callServerTool: async (call) => {
        calls.push(call);
        return {};
      },
    },
    engine: {
      zoenHostOnExport: (next) => {
        callback = next;
      },
      zoenHostRequest: async () => ({
        metadata: '{"layoutAccepted":true}',
        bytes: new Uint8Array(),
      }),
    },
  });
  await ui.layout({
    ...context,
    kind: "different-editor",
    revision: "guest-revision",
    capabilities: ["share:anything"],
    exportFormats: ["png"],
    itemId: "guest-item",
  });
  assert.equal(callback("png"), false);
  assert.equal(callback("pdf"), true);
  await tick();
  assert.deepEqual(calls, [
    { name: "app_binary_share", arguments: { id: "trusted-deck-item", format: "pdf" } },
  ]);
  ui.dispose();
});

test("shared owner labels only its fixed layout/export catch branches, not guest or exception metadata", async () => {
  let callback;
  const errors = [];
  const ui = installManagedHostUI({
    itemId: "owned-artifact",
    ready: () => true,
    onError: (error, source) => errors.push({ error, source }),
    app: { callServerTool: async () => ({ isError: true }) },
    engine: {
      zoenHostOnExport: (next) => {
        callback = next;
      },
      zoenHostRequest: async () => ({
        metadata: '{"layoutAccepted":false}',
        bytes: new Uint8Array(),
      }),
    },
  });
  await assert.rejects(
    ui.layout({ ...context, errorSource: "export" }),
    /acknowledge host geometry/,
  );
  assert.equal(errors.length, 1);
  assert.equal(errors[0].source, "layout");
  assert.equal(callback("pdf"), true);
  await tick();
  assert.equal(errors.length, 2);
  assert.equal(errors[1].source, "export");
  assert.match(errors[1].error.message, /native presentation share was refused/);
  ui.dispose();
});
