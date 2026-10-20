import assert from "node:assert/strict";
import { mkdtemp, readFile, writeFile, rm } from "node:fs/promises";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { fileURLToPath } from "node:url";
import test from "node:test";
import "../../../apps/zoen/src/i18n/test-setup.ts";
import { MiniappSession } from "../../../apps/zoen/src/miniapps/protocol.ts";
import { JSDOM, VirtualConsole } from "../../excalidraw-ui/node_modules/jsdom/lib/api.js";
import { buildApp } from "../../../tools/miniapp-build/build.mjs";
import { build } from "../../../tools/miniapp-build/node_modules/esbuild/lib/main.js";
import {
  findBrowserExecutable,
  ownedBrowserResult,
} from "../../excalidraw-ui/test/browser-driver.mjs";
import { withBrowserProfile } from "../../excalidraw-ui/test/browser-lifecycle.mjs";

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

test("real SDK and AppBridge keep four negotiated edges across theme-only, rotation and display-mode updates", async () => {
  const f = await fixture({ displayMode: "fullscreen", safeAreaInsets: initialInsets });
  try {
    await f.wait(() => f.vars()[0] === "72px");
    assert.deepEqual(f.vars(), ["72px", "9px", "24px", "7px"]);
    await f.context({ theme: "dark" });
    assert.equal(f.dom.window.document.documentElement.dataset.theme, "dark");
    assert.deepEqual(
      f.vars(),
      ["72px", "9px", "24px", "7px"],
      "partial theme cannot discard SDK-merged insets",
    );
    await f.context({
      safeAreaInsets: { top: 0, right: 32, bottom: 18, left: 28 },
      containerDimensions: { width: 760, height: 420 },
    });
    assert.deepEqual(f.vars(), ["0px", "32px", "18px", "28px"]);
    await f.context({ displayMode: "inline" });
    assert.deepEqual(f.vars(), ["0px", "0px", "0px", "0px"]);
    await f.context({ theme: "light" });
    assert.deepEqual(f.vars(), ["0px", "0px", "0px", "0px"]);
    await f.context({ displayMode: "fullscreen" });
    assert.deepEqual(f.vars(), ["0px", "32px", "18px", "28px"]);
    assert.equal(f.messages.filter((m) => m.method === "ui/initialize").length, 1);
    assert.deepEqual(f.calls, []);
    assert.deepEqual(f.errors, []);
  } finally {
    f.close();
  }
});

test("real context boundaries zero negative edges, preserve zero/fractional values and never offset inline content", async () => {
  const f = await fixture({
    displayMode: "fullscreen",
    safeAreaInsets: { top: -1, right: 0, bottom: 12.5, left: -0.5 },
  });
  try {
    await f.wait(() => f.vars()[2] === "12.5px");
    assert.deepEqual(f.vars(), ["0px", "0px", "12.5px", "0px"]);
    await f.context({ displayMode: "inline", safeAreaInsets: initialInsets });
    assert.deepEqual(f.vars(), ["0px", "0px", "0px", "0px"]);
    assert.deepEqual(f.errors, []);
  } finally {
    f.close();
  }
});

test("missing optional host insets use zero rather than inventing a native margin", async () => {
  const f = await fixture({ displayMode: "fullscreen" });
  try {
    assert.deepEqual(f.vars(), ["0px", "0px", "0px", "0px"]);
    assert.deepEqual(f.errors, []);
  } finally {
    f.close();
  }
});

test("nonfinite malformed host notifications cannot publish nonfinite CSS through the official transport", async () => {
  const f = await fixture({ displayMode: "fullscreen", safeAreaInsets: initialInsets });
  try {
    await f.wait(() => f.vars()[0] === "72px");
    for (const top of [NaN, Infinity, -Infinity]) {
      await f.context({ safeAreaInsets: { ...initialInsets, top } });
      // A subsequent valid notification fences delivery without replacing any safe-area field.
      await f.context({ theme: "dark" });
      await f.wait(() => f.dom.window.document.documentElement.dataset.theme === "dark");
      assert.deepEqual(
        f.vars(),
        ["72px", "9px", "24px", "7px"],
        "official schema rejects malformed numbers before the SDK callback",
      );
    }
    assert.ok(
      f.errors.every((message) =>
        /invalid|number|safeAreaInsets|NaN|Infinity|finite/i.test(message),
      ),
      JSON.stringify(f.errors),
    );
    assert.deepEqual(f.calls, []);
  } finally {
    f.close();
  }
});

test("actual SDK consumes one validated host close rectangle through AppBridge and clears inline, removed and retired chrome", async () => {
  const button = { top: 80, right: 21, width: 44, height: 44 };
  const f = await fixture({
    displayMode: "fullscreen",
    safeAreaInsets: initialInsets,
    containerDimensions: { width: 420, height: 760 },
    zoen: { chrome: { closeButton: button } },
  });
  const values = () =>
    ["top", "right", "width", "height"].map((edge) =>
      f.dom.window.document.documentElement.style.getPropertyValue("--zoen-host-close-" + edge),
    );
  try {
    await f.wait(() => values()[0] === "80px");
    assert.deepEqual(values(), ["80px", "21px", "44px", "44px"]);
    assert.deepEqual(f.vars(), ["72px", "9px", "24px", "7px"]);
    await f.context({ theme: "dark" });
    assert.deepEqual(values(), ["80px", "21px", "44px", "44px"]);
    await f.context({ displayMode: "inline" });
    assert.deepEqual(values(), ["", "", "", ""]);
    await f.context({ displayMode: "fullscreen" });
    assert.equal(values()[0], "80px");
    for (const malformed of [
      { ...button, width: 129 },
      { ...button, right: -1 },
      { ...button, top: "80" },
      { ...button, width: 0 },
      { ...button, right: 419 },
      { ...button, top: 750 },
    ]) {
      await f.context({ zoen: { chrome: { closeButton: malformed } } });
      assert.deepEqual(values(), ["", "", "", ""]);
    }
    await f.context({ zoen: { chrome: { closeButton: button } } });
    assert.equal(values()[0], "80px");
    await f.context({ zoen: { chrome: { closeButton: null } } });
    assert.deepEqual(values(), ["", "", "", ""]);
    await f.context({ zoen: { chrome: { closeButton: button } } });
    assert.equal(values()[0], "80px");
    f.dom.window.dispatchEvent(
      new f.dom.window.MessageEvent("message", {
        data: {
          jsonrpc: "2.0",
          id: "retire-sdk-chrome",
          method: "ui/resource-teardown",
          params: {},
        },
        source: f.dom.window.parent,
      }),
    );
    await tick();
    assert.deepEqual(values(), ["", "", "", ""]);
    await f.context({ zoen: { chrome: { closeButton: button } } });
    assert.deepEqual(
      values(),
      ["", "", "", ""],
      "a retired SDK cannot restore its old host corner",
    );
    assert.deepEqual(f.calls, []);
    assert.deepEqual(f.errors, []);
  } finally {
    f.close();
  }
});

// Full production HTML is built by the normal producer; only the host and measurement are fixtures.
const scheduleState = {
  title: "Owned safe-area schedule",
  business: "Owned safe-area schedule",
  timezone: "America/Sao_Paulo",
  currency: "BRL",
  accentHex: null,
  services: [{ id: "session", name: "Session", priceCents: 9000, minutes: 50 }],
  weeklyHours: Object.fromEntries([0, 1, 2, 3, 4, 5, 6].map((day) => [day, [["09:00", "12:00"]]])),
  slotMinutes: 30,
  horizonDays: 60,
  configured: true,
  bookings: [],
  events: [],
  busy: [],
  viewer: { id: "owner", role: "client" },
  next: 1,
};
const hikeState = {
  title: "Owned safe-area hike",
  area: "Bay Area",
  day: "Saturday",
  trails: [
    { id: "tomales", votes: [] },
    { id: "steep", votes: [] },
    { id: "lands", votes: [] },
  ],
  decided: null,
  itinerary: null,
  album: [],
};
for (const scenario of [
  { id: "list", width: 420, height: 760, state: listState, selector: ".builtin h1" },
  { id: "list", width: 340, height: 680, state: listState, selector: ".builtin h1" },
  {
    id: "list",
    width: 760,
    height: 420,
    state: listState,
    selector: ".builtin h1",
    insets: { top: 48, right: 32, bottom: 20, left: 32 },
  },
  { id: "schedule", width: 420, height: 760, state: scheduleState, selector: ".head" },
  ...[340, 420].map((width) => ({
    id: "schedule",
    width,
    height: 760,
    selector: ".head",
    font: "monospace",
    clippedTitle: true,
    state: {
      ...scheduleState,
      business:
        "WWWW long appointment schedule — one title must stay readable without covering the native close button",
    },
  })),
  { id: "hike", width: 420, height: 760, state: hikeState, selector: ".head" },
])
  test(`produced ${scenario.id} ${scenario.width}px${scenario.font ? " wide-font long title" : ""} controls clear host safe edges in real Chrome`, async (t) => {
    const chrome = await findBrowserExecutable();
    if (!chrome) {
      t.skip("Chrome unavailable locally; CI requires the normal sandboxed browser");
      return;
    }
    const insets = scenario.insets ?? initialInsets;
    const folder = await mkdtemp(join(tmpdir(), "zoen-builtin-safe-browser-"));
    await withBrowserProfile(folder, async () => {
      let html = scenario.id === "list" ? await listResource() : await produced(scenario.id);
      const probe = `(()=>{
      const errors=[];addEventListener('error',event=>errors.push(event.message));
      const rect=r=>({x:r.x,y:r.y,right:r.right,bottom:r.bottom,width:r.width,height:r.height});
      const box=e=>rect(e.getBoundingClientRect());
      const rawText=e=>{const range=document.createRange();range.selectNodeContents(e);return rect(range.getBoundingClientRect())};
      // DOM Range includes glyphs hidden by overflow/text-overflow. Apply only the
      // actual ancestor scrollport clips; a flex parent with overflow:visible does not clip.
      const visibleText=e=>{
        const output=[],walker=document.createTreeWalker(e,NodeFilter.SHOW_TEXT);
        for(let node=walker.nextNode();node;node=walker.nextNode()){
          if(!node.textContent.trim())continue;
          const range=document.createRange();range.selectNodeContents(node);
          for(const raw of range.getClientRects()){
            let part=rect(raw);
            for(let parent=node.parentElement;parent;parent=parent.parentElement){
              const css=getComputedStyle(parent);
              if(css.display==='none'||css.visibility!=='visible'||Number(css.opacity)===0){part=null;break;}
              const area=parent.getBoundingClientRect();
              if(/^(hidden|clip|scroll|auto)$/.test(css.overflowX)){
                part.x=Math.max(part.x,area.left+parseFloat(css.borderLeftWidth));
                part.right=Math.min(part.right,area.right-parseFloat(css.borderRightWidth));
              }
              if(/^(hidden|clip|scroll|auto)$/.test(css.overflowY)){
                part.y=Math.max(part.y,area.top+parseFloat(css.borderTopWidth));
                part.bottom=Math.min(part.bottom,area.bottom-parseFloat(css.borderBottomWidth));
              }
              if(part.right<=part.x||part.bottom<=part.y){part=null;break;}
            }
            if(part){part.width=part.right-part.x;part.height=part.bottom-part.y;output.push(part);}
          }
        }
        return output;
      };
      ${scenario.font ? `document.documentElement.style.setProperty('--z-font',${JSON.stringify(scenario.font)});` : ""}
      const measure=()=>{
        const element=document.querySelector(${JSON.stringify(scenario.selector)});
        if(!element||!element.textContent.includes(${JSON.stringify(scenario.state.business ?? scenario.state.title)})||document.documentElement.style.getPropertyValue('--zoen-host-safe-top')!==${JSON.stringify(insets.top + "px")}){requestAnimationFrame(measure);return;}
        requestAnimationFrame(()=>requestAnimationFrame(()=>parent.postMessage({qaSafe:true,title:element.textContent,header:box(element),text:rawText(element),visibleText:visibleText(element),label:box(element.querySelector('b')??element),labelText:rawText(element.querySelector('b')??element),frame:{width:innerWidth,height:innerHeight},body:box(document.body),bottom:document.querySelector('.bottom button')?box(document.querySelector('.bottom button')):null,edges:['top','right','bottom','left'].map(e=>document.documentElement.style.getPropertyValue('--zoen-host-safe-'+e)),errors},'*')));
      };requestAnimationFrame(measure);
    })();`;
      html = html.replace("</body>", `<script>${probe}</script></body>`);
      const host = `import{MiniappSession}from'./apps/zoen/src/miniapps/protocol.ts';
      const frame=document.createElement('iframe');frame.sandbox='allow-scripts';frame.style='border:0;width:${scenario.width}px;height:${scenario.height}px';
      const context={locale:'en',theme:'light',displayMode:'fullscreen',safeAreaInsets:${JSON.stringify(insets)},zoen:{chrome:{closeButton:{top:${insets.top + 8},right:${insets.right + 12},width:44,height:44}}}};
      const session=new MiniappSession({send:m=>frame.contentWindow.postMessage(m,'*'),initialize:()=>({hostInfo:{name:'Zoen owned host',version:'test'},hostCapabilities:{serverTools:{},experimental:{zoen:{native:[],services:['state','theme']}}},hostContext:context}),context:()=>context,state:()=>({content:[],structuredContent:${JSON.stringify(scenario.state)}}),tools:new Set(),resourceUri:'ui://builtin/${scenario.id}',resource:async()=>({contents:[]}),call:async()=>{throw Error('No tools during geometry')},native:async()=>({}),share:async()=>({}),openLink:async()=>({}),haptic(){},resize(){},close(){}});
      addEventListener('message',event=>{if(event.source!==frame.contentWindow)return;if(event.data?.qaSafe){const result=document.createElement('pre');result.id='result';result.textContent=JSON.stringify(event.data);document.body.append(result);}else void session.receive(JSON.stringify(event.data));});
      frame.srcdoc=${JSON.stringify(html)};document.body.append(frame);`;
      const bundle = await build({
        stdin: { contents: host, resolveDir: repo, loader: "js" },
        bundle: true,
        write: false,
        format: "esm",
        platform: "browser",
        define: { "process.env.NODE_ENV": '"production"' },
      });
      const path = join(folder, "host.html");
      await writeFile(
        path,
        `<!doctype html><body><script type="module">${bundle.outputFiles[0].text.replaceAll("</script", "<\\/script")}</script></body>`,
      );
      const browser = await ownedBrowserResult(chrome, folder, path);
      const result = JSON.parse(browser.text);
      assert.deepEqual(browser.errors, []);
      assert.deepEqual(result.errors, []);
      assert.deepEqual(
        result.edges,
        edges.map((edge) => insets[edge] + "px"),
      );
      assert.ok(
        result.title.includes(scenario.state.business ?? scenario.state.title),
        "actual hosted canonical state reached the production UI",
      );
      assert.ok(
        result.header.y >= insets.top,
        `header entered status area: ${JSON.stringify(result)}`,
      );
      assert.ok(result.header.x >= insets.left);
      assert.ok(result.header.right <= scenario.width - insets.right);
      const close = {
        x: scenario.width - insets.right - 12 - 44,
        y: insets.top + 8,
        right: scenario.width - insets.right - 12,
        bottom: insets.top + 8 + 44,
      };
      const overlap = (a, b) =>
        Math.max(0, Math.min(a.right, b.right) - Math.max(a.x, b.x)) *
        Math.max(0, Math.min(a.bottom, b.bottom) - Math.max(a.y, b.y));
      if (scenario.clippedTitle)
        t.diagnostic(
          JSON.stringify({
            width: scenario.width,
            font: scenario.font,
            rawText: result.text,
            label: result.label,
            labelText: result.labelText,
            visibleText: result.visibleText,
          }),
        );
      assert.ok(result.visibleText.length > 0, "production header has visible text");
      for (const fragment of result.visibleText) {
        assert.equal(
          overlap(fragment, close),
          0,
          "actual visible header text does not hide behind native X: " + JSON.stringify(result),
        );
      }
      if (scenario.clippedTitle) {
        assert.ok(
          result.labelText.right > result.label.right + 20,
          "long wide-font title actually exercises clipped glyphs",
        );
        assert.ok(
          result.label.right < close.x,
          "the actual title clipping box stays outside native close chrome",
        );
      }
      assert.equal(result.frame.width, scenario.width);
      assert.equal(result.frame.height, scenario.height);
      if (result.bottom) assert.ok(result.bottom.bottom <= scenario.height - insets.bottom);
    });
  });
