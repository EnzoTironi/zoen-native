import { build } from "esbuild";
import { JSDOM, VirtualConsole } from "jsdom";
import { fileURLToPath } from "node:url";
import { readFile } from "node:fs/promises";
import { png } from "../../../apps/zoen/src/core/drawing-review.fixtures.ts";
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
export async function exportToCanvas(input){
 (window.qaReviewExports??=[]).push(input);
 if(window.qaReviewGate)await window.qaReviewGate.promise;
 const text=JSON.stringify(input.elements);
 const encoded=text.includes('#ff0000')?window.qaReviewRed:window.qaReviewBlue;
 return {width:1,height:1,toBlob(cb){cb(new Blob([Uint8Array.from(atob(encoded),c=>c.charCodeAt(0))],{type:'image/png'}));}};
}
export const CaptureUpdateAction={NEVER:"NEVER"};
export const restore=(data)=>data;
export const serializeAsJSON=(elements,appState,files)=>JSON.stringify({type:'excalidraw',version:2,elements,appState:{viewBackgroundColor:appState.viewBackgroundColor}});
export function Excalidraw(props){
 window.qaProps=props;
 if(!window.qaScene)window.qaScene=props.initialData;
 const current=()=>window.qaScene;
 const change=()=>window.qaProps.onChange(current().elements,{...current().appState,viewModeEnabled:window.qaProps.viewModeEnabled},current().files);
 window.qaAcknowledge=change;
 window.qaEdit=(text)=>{if(window.qaProps.viewModeEnabled)return false;window.qaScene={...current(),elements:[{id:'text',type:'text',text}]};change();return true;};
 useEffect(()=>{props.excalidrawAPI({getSceneElements:()=>current().elements,getAppState:()=>({...current().appState,viewModeEnabled:window.qaProps.viewModeEnabled}),getFiles:()=>current().files,addFiles:(files)=>{window.qaScene={...current(),files:{...current().files,...Object.fromEntries(files.map(f=>[f.id,f]))}};},updateScene:(scene)=>{window.qaScene={...current(),elements:scene.elements,appState:scene.appState};if(!window.qaHoldApply)change();}});change();window.qaCanvasReady=true;},[]);
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
        p.onLoad({ filter: /src\/canonical-(?:app|canvas|serialize)\.tsx?$/ }, async ({ path }) => {
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
        p.onLoad({ filter: /src\/canonical-application\.ts$/ }, async ({ path }) => {
          let contents = await readFile(path, "utf8");
          if (process.env.ZOEN_CANONICAL_NEGATIVE === "refresh-apply-ack")
            contents = contents.replace(
              "await new Promise<void>((resolve, reject) => {",
              "await new Promise<void>((resolve, reject) => { queueMicrotask(resolve);",
            );
          return { contents, loader: "ts" };
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
  let bytes = sceneBytes(initial),
    version = 1;
  let nativeCall = null;
  const app = {
    getHostContext: () => ({ locale: "en" }),
    updateModelContext: async (value) => {
      statuses.push(value.structuredContent.zoen.sceneSave);
      return {};
    },
    async callServerTool({ name, arguments: a }) {
      calls.push({ name, args: a });
      if (nativeCall) return nativeCall(name, a);
      let value;
      if (name === "app_scene_read")
        value = {
          transferId: "read",
          version,
          currentVersion: version,
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
      w.Blob = Blob;
      w.qaReviewRed = png([255, 0, 0, 255]).toString("base64");
      w.qaReviewBlue = png([0, 0, 255, 255]).toString("base64");
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
    reviewExports: () => dom.window.qaReviewExports ?? [],
    holdReviewExport: () => {
      const gate = held();
      dom.window.qaReviewGate = gate;
      return gate;
    },
    setNativeCall: (call) => {
      nativeCall = call;
    },
    setReadScene: (json, nextVersion = 2) => {
      bytes = sceneBytes(json);
      version = nextVersion;
    },
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

export { fixture, completedNativeImage, pendingNativeImage, tick, held, scene, jpeg };
