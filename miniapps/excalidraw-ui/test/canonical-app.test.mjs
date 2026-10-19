import assert from "node:assert/strict";
import test from "node:test";
import { build } from "esbuild";
import { JSDOM, VirtualConsole } from "jsdom";
import { fileURLToPath } from "node:url";
import { readFile } from "node:fs/promises";
import { CHUNK_BYTES, sceneBytes, sceneHash } from "../src/canonical-transfer.ts";

const root = fileURLToPath(new URL("..", import.meta.url));
const pendingNativeImage = JSON.parse(
  await readFile(new URL("fixtures/native-pending-image.json", import.meta.url), "utf8"),
);
const jpeg = await readFile(new URL("fixtures/synthetic-beach.jpg", import.meta.url));
function completedNativeImage() {
  const scene = structuredClone(pendingNativeImage);
  Object.assign(scene.elements.at(-1), {
    fileId: "native_jpeg",
    status: "saved",
    width: 1200,
    height: 800,
  });
  scene.files.native_jpeg = {
    id: "native_jpeg",
    mimeType: "image/jpeg",
    dataURL: "data:image/jpeg;base64," + jpeg.toString("base64"),
    created: 1,
  };
  return scene;
}
const tick = () => new Promise((resolve) => setImmediate(resolve));
const held = () => {
  let resolve, reject;
  const promise = new Promise((yes, no) => {
    resolve = yes;
    reject = no;
  });
  return { promise, resolve, reject };
};
const scene = (text) =>
  JSON.stringify({
    type: "excalidraw",
    version: 2,
    elements: [{ id: "text", type: "text", text }],
    appState: { viewBackgroundColor: "#fff" },
    files: {},
  });
const graphics = `import React,{useEffect} from 'react';
export const serializeAsJSON=(elements,appState,files)=>JSON.stringify({type:'excalidraw',version:2,elements,appState:{viewBackgroundColor:appState.viewBackgroundColor}});
export function Excalidraw(props){
 window.qaProps=props;
 if(!window.qaScene)window.qaScene=props.initialData;
 const current=()=>window.qaScene;
 const change=()=>window.qaProps.onChange(current().elements,{...current().appState,viewModeEnabled:window.qaProps.viewModeEnabled},current().files);
 window.qaAcknowledge=change;
 window.qaEdit=(text)=>{if(window.qaProps.viewModeEnabled)return false;window.qaScene={...current(),elements:[{id:'text',type:'text',text}]};change();return true;};
 useEffect(()=>{props.excalidrawAPI({getSceneElements:()=>current().elements,getAppState:()=>({...current().appState,viewModeEnabled:window.qaProps.viewModeEnabled}),getFiles:()=>current().files});change();window.qaCanvasReady=true;},[]);
 useEffect(()=>{if(!window.qaHoldFreeze)change();},[props.viewModeEnabled]);
 return React.createElement('div',{'data-qa-canvas':props.viewModeEnabled?'readonly':'editable'});
}`;
const result = await build({
  absWorkingDir: root,
  stdin: {
    contents: `import React from 'react';import{createRoot}from'react-dom/client';import{CanonicalExcalidrawApp}from'./src/canonical-app';window.qaRoot=createRoot(document.body);window.qaRoot.render(<CanonicalExcalidrawApp app={window.qaApp} itemId="item"/>);`,
    resolveDir: root,
    loader: "tsx",
  },
  bundle: true,
  write: false,
  format: "iife",
  platform: "browser",
  jsx: "automatic",
  define: { "process.env.NODE_ENV": '"production"' },
  plugins: [
    {
      name: "graphics-only",
      setup(p) {
        p.onLoad({ filter: /src\/canonical-app\.tsx$/ }, async ({ path }) => {
          let contents = await readFile(path, "utf8");
          if (process.env.ZOEN_CANONICAL_NEGATIVE === "normal-flush")
            contents = contents.replace("if (action.current) return;", "");
          if (process.env.ZOEN_CANONICAL_NEGATIVE === "incomplete-image")
            contents = contents.replace(
              "if (!file?.dataURL) throw new PendingSceneImages();",
              "if (!file?.dataURL) continue;",
            );
          if (process.env.ZOEN_CANONICAL_NEGATIVE === "image-readiness")
            contents = contents.replace(
              "await Promise.all([saves.pause(), awaitImages()]);",
              "await saves.pause();",
            );
          if (process.env.ZOEN_CANONICAL_NEGATIVE === "teardown-freeze")
            contents = contents.replace(
              /await freeze\(["']teardown["']\);/,
              "await saves.pause();",
            );
          return { contents, loader: "tsx" };
        });
        p.onResolve({ filter: /^@excalidraw\/excalidraw$/ }, () => ({
          path: "graphics",
          namespace: "qa",
        }));
        p.onLoad({ filter: /.*/, namespace: "qa" }, () => ({
          contents: graphics,
          loader: "jsx",
          resolveDir: root,
        }));
      },
    },
  ],
});
async function fixture(initial = scene("initial")) {
  const calls = [],
    uploads = new Map(),
    failures = [],
    statuses = [],
    deadlines = new Map();
  let commitFailure = false,
    appendGate = null;
  const bytes = sceneBytes(initial);
  const app = {
    getHostContext: () => ({ locale: "en" }),
    updateModelContext: async (value) => {
      statuses.push(value.structuredContent.zoen.sceneSave);
      return {};
    },
    async callServerTool({ name, arguments: a }) {
      calls.push({ name, args: a });
      let value;
      if (name === "app_scene_read")
        value = {
          transferId: "read",
          bytes: bytes.length,
          sha256: sceneHash(bytes),
          chunkBytes: CHUNK_BYTES,
        };
      if (name === "app_scene_read_chunk")
        value = { index: a.index, data: Buffer.from(bytes).toString("base64") };
      if (name === "app_scene_begin") {
        const uploadId = "upload" + uploads.size;
        uploads.set(uploadId, { ...a, chunks: [] });
        value = { uploadId, chunkBytes: CHUNK_BYTES };
      }
      if (name === "app_scene_append") {
        uploads.get(a.uploadId).chunks[a.index] = Buffer.from(a.data, "base64");
        if (appendGate) await appendGate.promise;
        value = { nextIndex: a.index + 1 };
      }
      if (name === "app_scene_commit") {
        if (commitFailure) throw Error("shared conflict");
        value = {
          checkpointId: "item",
          version: 2,
          currentVersion: 2,
          sha256: uploads.get(a.uploadId).sha256,
        };
      }
      if (name === "app_scene_abort") value = {};
      return { content: [], structuredContent: value };
    },
  };
  const console = new VirtualConsole();
  console.on("jsdomError", (e) => failures.push(e.message));
  const dom = new JSDOM("<!doctype html><body></body>", {
    runScripts: "outside-only",
    pretendToBeVisual: true,
    virtualConsole: console,
    beforeParse(w) {
      w.qaApp = app;
      w.TextEncoder = TextEncoder;
      w.TextDecoder = TextDecoder;
      const nativeSetTimeout = w.setTimeout.bind(w),
        nativeClearTimeout = w.clearTimeout.bind(w);
      let id = -1;
      w.setTimeout = (work, delay, ...args) => {
        if (delay !== 10_000) return nativeSetTimeout(work, delay, ...args);
        deadlines.set(id, work);
        return id--;
      };
      w.clearTimeout = (timer) => {
        if (!deadlines.delete(timer)) nativeClearTimeout(timer);
      };
    },
  });
  dom.window.eval(result.outputFiles[0].text);
  const wait = async (predicate) => {
    for (let i = 0; i < 150; i++) {
      if (predicate()) return;
      await new Promise((r) => setTimeout(r, 5));
    }
    throw Error("Canvas did not settle: " + JSON.stringify(failures));
  };
  await wait(
    () =>
      dom.window.document.querySelector("[data-qa-canvas]") &&
      dom.window.qaCanvasReady &&
      typeof app.onteardown === "function",
  );
  await tick();
  return {
    app,
    dom,
    calls,
    uploads,
    failures,
    statuses,
    wait,
    setCommitFailure: (v) => {
      commitFailure = v;
    },
    holdAppend: () => {
      appendGate = held();
      return appendGate;
    },
    edit: (text) => dom.window.qaEdit(text),
    change(value) {
      dom.window.qaScene = structuredClone(value);
      dom.window.qaAcknowledge();
    },
    expireImports() {
      for (const work of deadlines.values()) work();
    },
    pointer: () =>
      dom.window.document
        .querySelector(".canonical-canvas")
        .dispatchEvent(new dom.window.Event("pointerup", { bubbles: true })),
    hide() {
      Object.defineProperty(dom.window.document, "visibilityState", {
        value: "hidden",
        configurable: true,
      });
      dom.window.document.dispatchEvent(new dom.window.Event("visibilitychange"));
    },
    close() {
      dom.window.qaRoot.unmount();
      dom.window.close();
    },
  };
}

test("actual canvas teardown freezes before latest capture, waits ACK, and only explicit resume restores editing", async () => {
  const f = await fixture();
  try {
    assert.equal(f.dom.window.qaProps.langCode, "en");
    assert.equal(f.calls.filter((c) => c.name === "app_scene_commit").length, 0);
    f.edit("latest before navigation");
    f.dom.window.qaHoldFreeze = true;
    const pending = f.app.onteardown({});
    let done = false;
    void pending.then(() => {
      done = true;
    });
    await tick();
    assert.equal(done, false);
    assert.equal(f.calls.filter((c) => c.name === "app_scene_commit").length, 0);
    await f.wait(() => f.dom.window.qaProps.viewModeEnabled);
    assert.equal(f.edit("hardware typing after freeze"), false);
    f.dom.window.qaAcknowledge();
    await pending;
    assert.equal(f.dom.window.qaProps.viewModeEnabled, true);
    assert.equal(f.calls.filter((c) => c.name === "app_scene_commit").length, 1);
    await f.app.onteardown({});
    assert.equal(f.calls.filter((c) => c.name === "app_scene_commit").length, 1);
    const saved = [...f.uploads.values()][0];
    assert.equal(
      JSON.parse(Buffer.concat(saved.chunks).toString()).elements[0].text,
      "latest before navigation",
    );
    f.hide();
    f.pointer();
    await tick();
    assert.equal(f.calls.filter((c) => c.name === "app_scene_commit").length, 1);
    await f.app.oncalltool({ name: "resume_scene" });
    await f.wait(() => !f.dom.window.qaProps.viewModeEnabled);
    assert.equal(f.edit("after resume"), true);
    assert.deepEqual(f.failures, []);
  } finally {
    f.close();
  }
});

test("actual held copy ignores hidden/pointer flushes and stays view-only without retrying old failed CAS", async () => {
  const f = await fixture();
  try {
    f.setCommitFailure(true);
    f.edit("failed A");
    await assert.rejects(f.app.onteardown({}), /shared conflict/);
    await f.wait(() => !f.dom.window.qaProps.viewModeEnabled);
    f.edit("newest B");
    const gate = f.holdAppend();
    const copy = f.app.oncalltool({ name: "prepare_scene_copy" });
    await f.wait(() => f.calls.some((c) => c.name === "app_scene_abort"));
    f.hide();
    f.pointer();
    await tick();
    assert.equal(f.calls.filter((c) => c.name === "app_scene_commit").length, 1);
    assert.equal(f.edit("late keyboard input"), false);
    gate.resolve();
    const result = await copy;
    const staged = f.uploads.get(result.structuredContent.uploadId);
    assert.equal(JSON.parse(Buffer.concat(staged.chunks).toString()).elements[0].text, "newest B");
    assert.equal(f.calls.filter((c) => c.name === "app_scene_commit").length, 1);
    assert.equal(f.dom.window.qaProps.viewModeEnabled, true);
    const before = f.calls.length;
    const replay = await f.app.oncalltool({ name: "prepare_scene_copy" });
    assert.deepEqual(replay, result);
    assert.equal(
      f.calls.length,
      before,
      "lost copy ACK retry returns exact staged result without abort/reupload",
    );
    await assert.rejects(f.app.oncalltool({ name: "resume_scene" }), /copy is already/);
    await assert.rejects(f.app.onteardown({}), /already in progress/);
    assert.equal(f.edit("still frozen"), false);
    assert.deepEqual(f.failures, []);
  } finally {
    f.close();
  }
});

test("actual failed teardown restores editing and preserves unsaved scene for exact retry", async () => {
  const f = await fixture();
  try {
    f.edit("retained");
    f.setCommitFailure(true);
    await assert.rejects(f.app.onteardown({}), /shared conflict/);
    await f.wait(() => !f.dom.window.qaProps.viewModeEnabled);
    assert.equal(f.dom.window.qaScene.elements[0].text, "retained");
    const mutation = f.calls.find((c) => c.name === "app_scene_begin").args.mutationId;
    f.setCommitFailure(false);
    await f.app.onteardown({});
    assert.equal(f.calls.filter((c) => c.name === "app_scene_begin")[1].args.mutationId, mutation);
    assert.equal(f.dom.window.qaProps.viewModeEnabled, true);
  } finally {
    f.close();
  }
});

test("actual canonical capture attaches only referenced live image bytes despite database serializer omitting files", async () => {
  const pixel = {
    id: "pixel",
    mimeType: "image/png",
    dataURL: "data:image/png;base64,iVBORw0KGgo=",
    created: 1,
  };
  const document = {
    type: "excalidraw",
    version: 2,
    elements: [{ id: "image", type: "image", fileId: "pixel", isDeleted: false }],
    appState: { viewBackgroundColor: "#fff" },
    files: { pixel, orphan: { ...pixel, id: "orphan" } },
  };
  const f = await fixture(JSON.stringify(document));
  try {
    f.dom.window.qaScene.appState.viewBackgroundColor = "#efe";
    await f.app.onteardown({});
    const saved = [...f.uploads.values()][0];
    assert.ok(saved);
    const value = JSON.parse(Buffer.concat(saved.chunks).toString());
    assert.deepEqual(value.files, { pixel });
    assert.equal(value.elements[0].fileId, "pixel");
  } finally {
    f.close();
  }
});

test("observed native pending JPEG never publishes an incomplete scene, and completion saves every element and owned byte", async () => {
  const f = await fixture();
  try {
    assert.equal(pendingNativeImage.elements.length, 6);
    assert.equal(pendingNativeImage.elements.at(-1).fileId, null);
    f.edit("older complete edit awaiting debounce");
    f.change(pendingNativeImage);
    f.pointer();
    f.hide();
    await new Promise((resolve) => setTimeout(resolve, 450));
    assert.equal(f.calls.filter((c) => c.name === "app_scene_begin").length, 0);
    assert.match(
      f.dom.window.document.querySelector('[role="status"]').textContent,
      /Importing image/,
    );
    assert.equal(f.statuses.at(-1).phase, "pending");
    f.change(completedNativeImage());
    await f.wait(() => f.calls.some((c) => c.name === "app_scene_commit"));
    await f.app.onteardown({});
    assert.equal(f.calls.filter((c) => c.name === "app_scene_commit").length, 1);
    const saved = JSON.parse(Buffer.concat([...f.uploads.values()][0].chunks).toString());
    assert.equal(saved.elements.length, 6);
    assert.equal(saved.elements.at(-1).fileId, "native_jpeg");
    assert.deepEqual(Buffer.from(saved.files.native_jpeg.dataURL.split(",")[1], "base64"), jpeg);
  } finally {
    f.close();
  }
});

for (const mode of ["teardown", "copy"]) {
  test(`actual ${mode} waits for slow native image completion before freezing and sending the whole scene`, async () => {
    const f = await fixture();
    try {
      f.change(pendingNativeImage);
      const request =
        mode === "teardown"
          ? f.app.onteardown({})
          : f.app.oncalltool({ name: "prepare_scene_copy" });
      let settled = false;
      void request.then(
        () => {
          settled = true;
        },
        () => {
          settled = true;
        },
      );
      await new Promise((resolve) => setTimeout(resolve, 25));
      assert.equal(settled, false);
      assert.equal(
        f.dom.window.qaProps.viewModeEnabled,
        false,
        "async upstream import callbacks remain enabled",
      );
      assert.equal(f.calls.filter((c) => c.name === "app_scene_begin").length, 0);
      await assert.rejects(
        mode === "copy" ? f.app.onteardown({}) : f.app.oncalltool({ name: "prepare_scene_copy" }),
        /already in progress/,
      );
      f.change(completedNativeImage());
      const reply = await request;
      assert.equal(f.dom.window.qaProps.viewModeEnabled, true);
      const upload =
        mode === "copy"
          ? f.uploads.get(reply.structuredContent.uploadId)
          : [...f.uploads.values()][0];
      const saved = JSON.parse(Buffer.concat(upload.chunks).toString());
      assert.equal(saved.elements.length, 6);
      assert.equal(
        saved.files.native_jpeg.dataURL,
        completedNativeImage().files.native_jpeg.dataURL,
      );
      assert.equal(
        f.calls.filter((c) => c.name === "app_scene_commit").length,
        mode === "copy" ? 0 : 1,
      );
    } finally {
      f.close();
    }
  });
}

test("import deadline retains editable pending scene; explicit retry after completion does not inherit an invalid mutation", async () => {
  const f = await fixture();
  try {
    f.change(pendingNativeImage);
    const teardown = f.app.onteardown({});
    await tick();
    f.expireImports();
    await assert.rejects(teardown, /still importing/);
    assert.equal(f.dom.window.qaProps.viewModeEnabled, false);
    assert.equal(f.dom.window.qaScene.elements.at(-1).fileId, null);
    assert.equal(f.calls.filter((c) => c.name === "app_scene_begin").length, 0);
    await f.wait(() =>
      /still importing/.test(f.dom.window.document.querySelector('[role="status"]').textContent),
    );
    f.change(completedNativeImage());
    await f.app.onteardown({});
    assert.equal(f.calls.filter((c) => c.name === "app_scene_commit").length, 1);
    assert.deepEqual(f.failures, []);
  } finally {
    f.close();
  }
});

test("unmount cancels an import-waiting action without a late freeze or publication", async () => {
  const f = await fixture();
  f.change(pendingNativeImage);
  const request = f.app.onteardown({});
  const rejected = assert.rejects(request, /closed/);
  await tick();
  f.close();
  await rejected;
  assert.equal(f.calls.filter((c) => c.name === "app_scene_begin").length, 0);
});

test("unmount drains an import's fire-and-forget pause behind an admitted save without an unhandled close rejection", async () => {
  const f = await fixture();
  const gate = f.holdAppend();
  f.edit("admitted complete scene");
  f.pointer();
  await f.wait(() => f.calls.some((c) => c.name === "app_scene_append"));
  f.change(pendingNativeImage);
  f.close();
  gate.resolve();
  await tick();
  await tick();
  assert.equal(f.calls.filter((c) => c.name === "app_scene_commit").length, 0);
});

test("import deadline starts while an older save is awaiting ACK; its late completion cannot freeze the editor", async () => {
  const f = await fixture();
  const gate = f.holdAppend();
  try {
    f.edit("older admitted scene");
    f.pointer();
    await f.wait(() => f.calls.some((c) => c.name === "app_scene_append"));
    f.change(pendingNativeImage);
    const teardown = f.app.onteardown({});
    await tick();
    f.expireImports();
    await assert.rejects(teardown, /still importing/);
    assert.equal(f.dom.window.qaProps.viewModeEnabled, false);
    gate.resolve();
    await f.wait(() => f.calls.some((c) => c.name === "app_scene_commit"));
    await tick();
    assert.equal(f.dom.window.qaProps.viewModeEnabled, false);
    assert.equal(f.dom.window.qaScene.elements.at(-1).fileId, null);
  } finally {
    gate.resolve();
    f.close();
  }
});

test("pending import preserves an earlier ambiguous attempt and its exact retry before the complete latest scene", async () => {
  const f = await fixture();
  try {
    f.edit("ambiguous old edit");
    f.setCommitFailure(true);
    await assert.rejects(f.app.onteardown({}), /shared conflict/);
    await f.wait(() => !f.dom.window.qaProps.viewModeEnabled);
    const failure = f.statuses.find((value) => value.phase === "error").error;
    const mutation = f.calls.find((c) => c.name === "app_scene_begin").args.mutationId;
    f.change(pendingNativeImage);
    await tick();
    assert.ok(f.dom.window.document.querySelector('[role="status"]').textContent.includes(failure));
    assert.equal(f.statuses.at(-1).phase, "error");
    const waiting = f.app.onteardown({});
    await tick();
    f.expireImports();
    await assert.rejects(waiting, /still importing/);
    await tick();
    assert.ok(f.dom.window.document.querySelector('[role="status"]').textContent.includes(failure));
    f.change(completedNativeImage());
    await tick();
    assert.equal(f.calls.filter((c) => c.name === "app_scene_begin").length, 1);
    f.setCommitFailure(false);
    await f.app.onteardown({});
    const writes = f.calls.filter((c) => c.name === "app_scene_begin");
    assert.equal(writes.length, 3);
    assert.equal(writes[1].args.mutationId, mutation);
    assert.notEqual(writes[2].args.mutationId, mutation);
    assert.equal(
      JSON.parse(Buffer.concat([...f.uploads.values()][2].chunks).toString()).elements.length,
      6,
    );
  } finally {
    f.close();
  }
});

test("image becoming incomplete during positive freeze ACK rejects without publishing or dropping it", async () => {
  const f = await fixture();
  try {
    f.edit("before import");
    f.dom.window.qaHoldFreeze = true;
    const teardown = f.app.onteardown({});
    await f.wait(() => f.dom.window.qaProps.viewModeEnabled);
    f.change(pendingNativeImage);
    await assert.rejects(teardown, /still importing/);
    await f.wait(() => !f.dom.window.qaProps.viewModeEnabled);
    assert.equal(f.calls.filter((c) => c.name === "app_scene_begin").length, 0);
    assert.equal(f.dom.window.qaScene.elements.at(-1).fileId, null);
  } finally {
    f.close();
  }
});
