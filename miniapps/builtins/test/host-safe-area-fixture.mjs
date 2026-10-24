import { mkdtemp, readFile, rm } from "node:fs/promises";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { fileURLToPath } from "node:url";
import "../../../apps/zoen/src/i18n/test-setup.ts";
import { MiniappSession } from "../../../apps/zoen/src/miniapps/protocol.ts";
import { JSDOM, VirtualConsole } from "../../excalidraw-ui/node_modules/jsdom/lib/api.js";
import { buildApp } from "../../../tools/miniapp-build/build.mjs";

const repo = fileURLToPath(new URL("../../../", import.meta.url));
const edges = ["top", "right", "bottom", "left"];
const initialInsets = { top: 72, right: 9, bottom: 24, left: 7 };
const listState = {
  title: "Owned safe-area list",
  items: [{ id: "water", text: "Water", done: false, by: null }],
  log: [],
};
let listHtml;
async function produced(id = "list") {
  const folder = await mkdtemp(join(tmpdir(), "zoen-safe-built-"));
  try {
    const source = id === "list" ? join(repo, "miniapps/builtins") : join(repo, "miniapps", id);
    await buildApp(source, folder, id === "list" ? { manifestPath: "manifests/list.json" } : {});
    return await readFile(join(folder, `${id}.html`), "utf8");
  } finally {
    await rm(folder, { recursive: true, force: true });
  }
}
const listResource = () => (listHtml ??= produced());
const tick = () => new Promise(setImmediate);
async function fixture(context) {
  const html = await listResource();
  let session, parent;
  const errors = [],
    calls = [],
    messages = [];
  const console = new VirtualConsole();
  console.on("jsdomError", (error) => errors.push(error.message));
  console.on("error", (...values) => errors.push(values.map(String).join(" ")));
  const dom = new JSDOM(html, {
    runScripts: "dangerously",
    pretendToBeVisual: true,
    virtualConsole: console,
    beforeParse(window) {
      window.TextEncoder = TextEncoder;
      window.TextDecoder = TextDecoder;
      window.matchMedia = () => ({
        matches: false,
        addEventListener() {},
        removeEventListener() {},
      });
      window.ResizeObserver = class {
        observe() {}
        disconnect() {}
      };
      parent = {
        postMessage(message) {
          messages.push(message);
          void session.receive(JSON.stringify(message));
        },
      };
      session = new MiniappSession({
        send(message) {
          window.dispatchEvent(
            new window.MessageEvent("message", { data: message, source: parent }),
          );
        },
        initialize: () => ({
          hostInfo: { name: "Zoen owned host", version: "test" },
          hostCapabilities: {
            serverTools: {},
            experimental: { zoen: { native: [], services: ["state", "theme"] } },
          },
          hostContext: { locale: "en", theme: "light", ...context },
        }),
        context: () => context,
        state: () => ({ content: [], structuredContent: listState }),
        tools: new Set(["list_add", "list_toggle", "list_get_state"]),
        resourceUri: "ui://builtin/list",
        resource: async () => ({ contents: [] }),
        call: async (name) => {
          calls.push(name);
          throw Error("Opening geometry must not call a domain tool");
        },
        native: async () => ({}),
        share: async () => ({}),
        openLink: async () => ({}),
        haptic() {},
        resize() {},
        close() {},
      });
      Object.defineProperty(window, "parent", { value: parent });
    },
  });
  const wait = async (predicate) => {
    const end = Date.now() + 3000;
    while (!predicate()) {
      if (Date.now() > end)
        throw Error(
          "Actual List/SDK context did not arrive: " +
            dom.window.document.body.textContent.slice(0, 200),
        );
      await new Promise((r) => setTimeout(r, 5));
    }
  };
  const vars = () =>
    edges.map((edge) =>
      dom.window.document.documentElement.style.getPropertyValue(`--zoen-host-safe-${edge}`),
    );
  await wait(() => dom.window.document.querySelector("h1")?.textContent === listState.title);
  return {
    dom,
    errors,
    calls,
    messages,
    vars,
    wait,
    async context(delta) {
      dom.window.dispatchEvent(
        new dom.window.MessageEvent("message", {
          data: { jsonrpc: "2.0", method: "ui/notifications/host-context-changed", params: delta },
          source: parent,
        }),
      );
      await tick();
    },
    close() {
      session.dispose();
      dom.window.close();
    },
  };
}


export { listResource, listState, fixture, edges, initialInsets, produced, repo, tick };
