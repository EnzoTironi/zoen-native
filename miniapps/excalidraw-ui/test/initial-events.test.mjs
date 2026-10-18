import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
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
export const serializeAsJSON=JSON.stringify;
export async function exportToSvg({elements}){
 const svg=document.createElementNS('http://www.w3.org/2000/svg','svg');
 svg.setAttribute('viewBox','0 0 300 200');
 svg.setAttribute('data-qa-shapes',JSON.stringify(elements));
 return svg;
}
export function Excalidraw(props){
 window.qaEditorElements=props.initialData.elements;
 useEffect(()=>{props.excalidrawAPI?.({getSceneElements:()=>props.initialData.elements,updateScene:()=>{}})},[]);
 return React.createElement('div',{'data-qa-editor':'ready'});
}
export function MainMenu(){return null;}
MainMenu.Item=()=>null;`;

async function compile(mutate = (text) => text) {
  const result = await build({
    absWorkingDir: root,
    stdin: {
      contents:
        "import {createRoot} from 'react-dom/client';import {ExcalidrawApp} from './src/mcp-app';createRoot(document.body).render(<ExcalidrawApp/>);",
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

async function fixture({ mode = "fullscreen", mutate, earlyContext } = {}) {
  const runtime = await compile(mutate);
  const failures = [];
  const keys = [];
  const early = [];
  let session;
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
  const dom = new JSDOM("<!doctype html><body></body>", {
    runScripts: "outside-only",
    pretendToBeVisual: true,
    virtualConsole: console,
    beforeParse(window) {
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
      const parent = {
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
          hostCapabilities: { serverTools: {} },
          hostContext: { displayMode: mode, containerDimensions: { width: 400, height: 700 } },
        }),
        arguments: () => ({ elements: JSON.stringify(elements) }),
        state: () => ({ content: [], structuredContent: { checkpointId: "qa-checkpoint" } }),
        context: () => ({ displayMode: mode, containerDimensions: { width: 400, height: 700 } }),
        tools: new Set(["read_checkpoint", "save_checkpoint"]),
        resourceUri: "ui://zoen/excalidraw.html",
        resource: async () => ({ contents: [] }),
        call: async () => ({ content: [] }),
        openLink: async () => ({}),
        share: async () => ({}),
        native: async () => ({}),
        haptic() {},
        resize() {},
        close() {},
      });
    },
  });
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
    wait,
    close() {
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
