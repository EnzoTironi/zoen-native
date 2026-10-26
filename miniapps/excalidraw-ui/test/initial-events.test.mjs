import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { createHash } from "node:crypto";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";
import { build } from "esbuild";
import { JSDOM, VirtualConsole } from "jsdom";
import { MiniappSession } from "../../../apps/zoen/src/miniapps/protocol.ts";

const root = dirname(dirname(fileURLToPath(import.meta.url)));
const source = await readFile(join(root, "src/mcp-app.tsx"), "utf8");
const graphicFixture = `import React, {useEffect} from 'react';
export const FONT_FAMILY={Excalifont:5};
export const CaptureUpdateAction={NEVER:'never'};
export const convertToExcalidrawElements=(items)=>items;
export const restore=(value)=>value;
export const serializeAsJSON=(elements,appState)=>JSON.stringify({type:"excalidraw",version:2,elements,appState});
export async function exportToSvg({elements}){
 const svg=document.createElementNS('http://www.w3.org/2000/svg','svg');
 svg.setAttribute('viewBox','0 0 300 200');
 svg.setAttribute('data-qa-shapes',JSON.stringify(elements));
 return svg;
}
export function Excalidraw(props){
 window.qaEditorElements=props.initialData.elements;
 useEffect(()=>{props.excalidrawAPI?.({getSceneElements:()=>props.initialData.elements,getAppState:()=>props.initialData.appState??{},getFiles:()=>props.initialData.files??{},updateScene:()=>{}});props.onChange?.(props.initialData.elements,props.initialData.appState??{},props.initialData.files??{});},[]);
 return React.createElement('div',{'data-qa-editor':'ready'});
}
export function MainMenu(){return null;}
MainMenu.Item=()=>null;`;

async function compile(mutate = (text) => text) {
  const result = await build({
    absWorkingDir: root,
    stdin: {
      contents:
        "import {createRoot} from 'react-dom/client';import {ExcalidrawApp} from './src/mcp-app';window.qaRoot=createRoot(document.body);window.qaRoot.render(<ExcalidrawApp/>);",
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
        name: "graphics-only-fixture",
        setup(plugin) {
          plugin.onLoad({ filter: /\/src\/mcp-app\.tsx$/ }, () => ({
            contents: mutate(source),
            loader: "tsx",
          }));
          plugin.onResolve({ filter: /^@excalidraw\/excalidraw$/ }, () => ({
            path: "drawing-library",
            namespace: "qa-graphics",
          }));
          plugin.onLoad({ filter: /.*/, namespace: "qa-graphics" }, () => ({
            contents: graphicFixture,
            loader: "jsx",
            resolveDir: root,
          }));
          plugin.onLoad({ filter: /\.css$/ }, () => ({ contents: "", loader: "js" }));
        },
      },
    ],
  });
  return result.outputFiles[0].text;
}

async function fixture({
  mode = "fullscreen",
  mutate,
  earlyContext,
  canonical = false,
  initialContext = {},
  previousTop,
} = {}) {
  const runtime = await compile(mutate);
  const failures = [];
  const keys = [];
  const early = [];
  const sizes = [];
  let session, parent;
  const console = new VirtualConsole();
  console.on("jsdomError", (error) => failures.push(error.message));
  const elements = [
    {
      id: "idea",
      type: "rectangle",
      x: 0,
      y: 0,
      width: 240,
      height: 120,
      label: { text: "Exact initial idea" },
    },
  ];
  let context = {
    displayMode: mode,
    containerDimensions: { width: 400, height: 700 },
    ...initialContext,
  };
  const initialScene = JSON.stringify({
    type: "excalidraw",
    version: 2,
    elements,
    appState: {},
    files: {},
  });
  const sceneBytes = new TextEncoder().encode(initialScene);
  const sceneHash = createHash("sha256").update(sceneBytes).digest("hex");
  const dom = new JSDOM("<!doctype html><body></body>", {
    runScripts: "outside-only",
    pretendToBeVisual: true,
    virtualConsole: console,
    beforeParse(window) {
      window.TextEncoder = TextEncoder;
      window.TextDecoder = TextDecoder;
      window.matchMedia = () => ({
        matches: true,
        addEventListener() {},
        removeEventListener() {},
      });
      window.ResizeObserver = class {
        observe() {}
        disconnect() {}
      };
      Object.defineProperty(window.document, "fonts", {
        value: { ready: Promise.resolve(), load: async () => [] },
      });
      Object.defineProperty(window, "localStorage", {
        value: {
          getItem(key) {
            keys.push(key);
            return null;
          },
          setItem() {},
        },
      });
      parent = {
        postMessage(message) {
          void session.receive(JSON.stringify(message));
        },
      };
      Object.defineProperty(window, "parent", { value: parent });
      session = new MiniappSession({
        send(message) {
          if (
            message.method === "ui/notifications/tool-input" ||
            message.method === "ui/notifications/tool-result"
          )
            early.push({
              method: message.method,
              coreMounted: !!window.document.querySelector("main"),
            });
          window.dispatchEvent(
            new window.MessageEvent("message", { data: message, source: parent }),
          );
          if (message.method === "ui/notifications/tool-result" && earlyContext)
            window.dispatchEvent(
              new window.MessageEvent("message", {
                data: {
                  jsonrpc: "2.0",
                  method: "ui/notifications/host-context-changed",
                  params: earlyContext,
                },
                source: parent,
              }),
            );
        },
        initialize: () => ({
          hostInfo: { name: "Zoen", version: "qa" },
          hostCapabilities: {
            serverTools: {},
            ...(canonical
              ? { experimental: { zoen: { canonicalScene: { version: 1, itemId: "item" } } } }
              : {}),
          },
          hostContext: context,
        }),
        arguments: () => ({ elements: JSON.stringify(elements) }),
        state: () => ({ content: [], structuredContent: { checkpointId: "qa-checkpoint" } }),
        context: () => context,
        tools: new Set([
          "read_checkpoint",
          "save_checkpoint",
          "app_scene_read",
          "app_scene_read_chunk",
        ]),
        resourceUri: "ui://zoen/excalidraw.html",
        resource: async () => ({ contents: [] }),
        call: async (name) => ({
          content: [],
          ...(name === "app_scene_read"
            ? {
                structuredContent: {
                  transferId: "read",
                  version: 1,
                  currentVersion: 1,
                  bytes: sceneBytes.length,
                  sha256: sceneHash,
                  chunkBytes: 196608,
                },
              }
            : name === "app_scene_read_chunk"
              ? {
                  structuredContent: { index: 0, data: Buffer.from(sceneBytes).toString("base64") },
                }
              : {}),
        }),
        openLink: async () => ({}),
        share: async () => ({}),
        native: async () => ({}),
        haptic() {},
        resize(height) {
          sizes.push(height);
        },
        close() {},
      });
    },
  });
  if (previousTop) dom.window.document.documentElement.style.setProperty("--sat", previousTop);
  dom.window.eval(runtime);
  const wait = async (predicate) => {
    const end = Date.now() + 3000;
    while (!predicate()) {
      if (Date.now() > end)
        throw new Error(
          `Editor did not receive initial data. ${JSON.stringify({ early, failures, text: dom.window.document.body.textContent.slice(0, 160) })}`,
        );
      await new Promise((resolve) => setTimeout(resolve, 10));
    }
  };
  return {
    dom,
    early,
    failures,
    keys,
    elements,
    sizes,
    wait,
    context(delta) {
      // Exercise a genuinely partial wire notification, not a fixture-side merged snapshot.
      dom.window.dispatchEvent(
        new dom.window.MessageEvent("message", {
          data: { jsonrpc: "2.0", method: "ui/notifications/host-context-changed", params: delta },
          source: parent,
        }),
      );
      return new Promise((resolve) => setImmediate(resolve));
    },
    unmount() {
      dom.window.qaRoot.unmount();
    },
    close() {
      dom.window.qaRoot.unmount();
      session.dispose();
      dom.window.close();
    },
  };
}

test("actual upstream React UI buffers real AppBridge launch events before mounting, including checkpoint and fullscreen", async () => {
  const f = await fixture();
  try {
    await f.wait(() => f.dom.window.document.querySelector("[data-qa-editor]"));
    assert.deepEqual(
      f.early.map((entry) => entry.coreMounted),
      [false, false],
      "host delivery must precede the Core effect to reproduce the race",
    );
    assert.deepEqual(
      JSON.parse(JSON.stringify(f.dom.window.qaEditorElements)).map(
        ({ id, type, x, y, width, height, label }) => ({
          id,
          type,
          x,
          y,
          width,
          height,
          label: { text: label.text },
        }),
      ),
      f.elements,
    );
    assert.ok(
      f.keys.includes("excalidraw:qa-checkpoint"),
      "initial result reaches actual checkpoint handler",
    );
    assert.deepEqual(f.failures, []);
  } finally {
    f.close();
  }
});

test("host context arriving before Core mount is retained without host launch replay", async () => {
  const f = await fixture({
    mode: "inline",
    earlyContext: { displayMode: "fullscreen", containerDimensions: { width: 400, height: 680 } },
  });
  try {
    await f.wait(() => f.dom.window.document.querySelector("[data-qa-editor]"));
    assert.equal(f.early.length, 2);
    assert.deepEqual(
      JSON.parse(JSON.stringify(f.dom.window.qaEditorElements)).map(
        ({ id, type, x, y, width, height, label }) => ({
          id,
          type,
          x,
          y,
          width,
          height,
          label: { text: label.text },
        }),
      ),
      f.elements,
    );
  } finally {
    f.close();
  }
});

// Negative controls compile the real source with only the relevant repair removed.
// A timeout here is expected evidence of the reproduced failure, not an acceptance pass.
test("negative: prior unbuffered registration loses the same early launch", async () => {
  const f = await fixture({
    mutate: (text) => text.replace("onAppCreated: bufferInitialToolEvents,", ""),
  });
  try {
    await assert.rejects(
      f.wait(() => f.dom.window.document.querySelector("[data-qa-editor]")),
      /did not receive initial data/,
    );
    assert.deepEqual(
      f.early.map((entry) => entry.coreMounted),
      [false, false],
    );
    assert.equal(f.dom.window.qaEditorElements, undefined);
  } finally {
    f.close();
  }
});

test("negative: inline-only initial state does not enter the already-negotiated fullscreen editor", async () => {
  const f = await fixture({
    mutate: (text) =>
      text.replace(
        /const \[displayMode, setDisplayMode\] = useState<"inline" \| "fullscreen">\(\(\) =>\n    app\.getHostContext\(\)\?\.displayMode === "fullscreen" \? "fullscreen" : "inline"\n  \);/,
        'const [displayMode, setDisplayMode] = useState<"inline" | "fullscreen">("inline");',
      ),
  });
  try {
    await assert.rejects(
      f.wait(() => f.dom.window.document.querySelector("[data-qa-editor]")),
      /did not receive initial data/,
    );
    assert.ok(
      f.keys.includes("excalidraw:qa-checkpoint"),
      "buffer remains active in this independent control",
    );
  } finally {
    f.close();
  }
});

for (const canonical of [false, true]) {
  test(`${canonical ? "canonical" : "legacy"} editor uses negotiated host safe area and retains partial updates without resizing canvas`, async () => {
    const f = await fixture({
      canonical,
      previousTop: "3px",
      initialContext: { safeAreaInsets: { top: 31, right: 9, bottom: 22, left: 7 } },
    });
    const root = f.dom.window.document.documentElement;
    const values = () =>
      ["--sat", "--sar", "--sab", "--sal"].map((key) => root.style.getPropertyValue(key));
    try {
      await f.wait(() => f.dom.window.document.querySelector("[data-qa-editor]"));
      assert.deepEqual(values(), ["31px", "9px", "22px", "7px"]);
      // A theme-only notification must not discard the SDK's previously merged insets.
      await f.context({ theme: "dark" });
      await f.wait(() => values()[0] === "31px");
      await f.context({
        safeAreaInsets: { top: 0, right: 42, bottom: 18, left: 40 },
        containerDimensions: { width: 900, height: 400 },
      });
      await f.wait(() => values()[1] === "42px");
      assert.deepEqual(
        values(),
        ["0px", "42px", "18px", "40px"],
        "rotated/wide controls still respect physical edges",
      );
      const canvas = f.dom.window.document.querySelector(canonical ? ".canonical-canvas" : ".main");
      if (!canonical) await f.wait(() => canvas.style.height === "400px");
      assert.equal(canvas.style.padding, "");
      assert.equal(canvas.style.margin, "");
      await f.context({ displayMode: "inline" });
      await f.wait(() => values().every((value) => value === ""));
      await f.context({ displayMode: "fullscreen" });
      await f.wait(() => values()[1] === "42px");
      // Let the real SDK's initial auto-resize frame finish while its transport is open.
      await f.wait(() => f.sizes.length > 0);
      f.unmount();
      assert.deepEqual(values(), ["3px", "", "", ""], "cleanup restores pre-existing declarations");
      await f.context({ safeAreaInsets: { top: 99, right: 99, bottom: 99, left: 99 } });
      await new Promise((resolve) => setTimeout(resolve, 20));
      assert.deepEqual(values(), ["3px", "", "", ""], "closed root removed only its own listener");
      assert.deepEqual(f.failures, []);
    } finally {
      f.close();
    }
  });
}

test("host safe area arriving before either editor mounts is retained by the real MCP App", async () => {
  const f = await fixture({
    canonical: true,
    earlyContext: { safeAreaInsets: { top: 35, right: 1, bottom: 24, left: 2 } },
  });
  try {
    await f.wait(() => f.dom.window.document.querySelector("[data-qa-editor]"));
    assert.equal(f.dom.window.document.documentElement.style.getPropertyValue("--sat"), "35px");
    assert.equal(f.dom.window.document.documentElement.style.getPropertyValue("--sab"), "24px");
    assert.equal(f.early.length, 2, "no fabricated second launch is needed");
  } finally {
    f.close();
  }
});

test("negative: removing the shared subscription reproduces missing canonical safe-area controls", async () => {
  const f = await fixture({
    canonical: true,
    initialContext: { safeAreaInsets: { top: 31, right: 9, bottom: 22, left: 7 } },
    mutate: (text) => {
      assert.equal(text.split("useHostSafeArea(app);").length, 2);
      return text.replace("useHostSafeArea(app);", "");
    },
  });
  try {
    await f.wait(() =>
      f.dom.window.document.querySelector("[data-canonical-editor] [data-qa-editor]"),
    );
    assert.equal(f.dom.window.document.documentElement.style.getPropertyValue("--sat"), "");
    assert.equal(f.dom.window.document.documentElement.style.getPropertyValue("--sab"), "");
  } finally {
    f.close();
  }
});

for (const canonical of [false, true])
  test(`${canonical ? "canonical" : "legacy"} actual bridge binds, updates and clears one native close rectangle without replacing content`, async () => {
    const f = await fixture({
      canonical,
      initialContext: {
        safeAreaInsets: { top: 31, right: 9, bottom: 22, left: 7 },
        zoen: { chrome: { closeButton: { top: 39, right: 21, width: 44, height: 44 } } },
      },
    });
    const root = f.dom.window.document.documentElement;
    const values = () =>
      ["top", "right", "width", "height"].map((edge) =>
        root.style.getPropertyValue("--zoen-host-close-" + edge),
      );
    try {
      await f.wait(() => f.dom.window.document.querySelector("[data-qa-editor]"));
      assert.deepEqual(values(), ["39px", "21px", "44px", "44px"]);
      const editor = f.dom.window.document.querySelector("[data-qa-editor]");
      const originalElements = JSON.stringify(f.dom.window.qaEditorElements);
      await f.context({ theme: "dark" });
      assert.deepEqual(values(), ["39px", "21px", "44px", "44px"]);
      await f.context({
        safeAreaInsets: { top: 0, right: 42, bottom: 18, left: 40 },
        containerDimensions: { width: 900, height: 400 },
        zoen: { chrome: { closeButton: { top: 8, right: 54, width: 44, height: 44 } } },
      });
      assert.deepEqual(values(), ["8px", "54px", "44px", "44px"]);
      assert.equal(root.style.getPropertyValue("--sat"), "0px");
      assert.strictEqual(
        f.dom.window.document.querySelector("[data-qa-editor]"),
        editor,
        "chrome/geometry updates preserve the current editor",
      );
      await f.context({ displayMode: "inline" });
      assert.deepEqual(values(), ["", "", "", ""]);
      await f.context({ displayMode: "fullscreen", zoen: { chrome: { closeButton: null } } });
      assert.deepEqual(values(), ["", "", "", ""]);
      await f.context({
        zoen: { chrome: { closeButton: { top: 8, right: 54, width: 44, height: 44 } } },
      });
      assert.equal(values()[0], "8px");
      // Legacy inline mode intentionally replaces the editor with its SVG preview.
      // Returning to fullscreen may mount a new editor; the document must remain exact.
      await f.wait(() => f.dom.window.document.querySelector("[data-qa-editor]"));
      assert.equal(JSON.stringify(f.dom.window.qaEditorElements), originalElements);
      await f.wait(() => f.sizes.length > 0);
      f.unmount();
      assert.deepEqual(values(), ["", "", "", ""]);
      await f.context({
        zoen: { chrome: { closeButton: { top: 99, right: 12, width: 44, height: 44 } } },
      });
      assert.deepEqual(values(), ["", "", "", ""]);
      assert.deepEqual(f.failures, []);
    } finally {
      f.close();
    }
  });
