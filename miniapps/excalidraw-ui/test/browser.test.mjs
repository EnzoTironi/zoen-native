import assert from "node:assert/strict";
import { findBrowserExecutable, ownedBrowserResult } from "./browser-driver.mjs";
import { mkdir, mkdtemp, readFile, writeFile } from "node:fs/promises";
import { withBrowserProfile } from "./browser-lifecycle.mjs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";
import { build } from "esbuild";
import { bundledLibraryAssets } from "../library-assets.mjs";
import { sandboxHtml } from "../../../apps/zoen/src/miniapps/sandbox.ts";

const root = fileURLToPath(new URL("..", import.meta.url));
test("real production canvas boots with no network authority; hidden Mermaid menu/paste retain ordinary Unicode text", async (t) => {
  const chrome = await findBrowserExecutable();
  if (!chrome) {
    t.skip("Chrome unavailable locally; install Chrome/Chromium or set CHROME_BIN. Native WebKit acceptance remains separate.");
    return;
  }
  const folder = await mkdtemp(join(tmpdir(), "zoen-canonical-browser-"));
  await withBrowserProfile(folder, async () => {
    let html = await readFile(
      join(root, "../../crates/miniapps/resources/excalidraw.html"),
      "utf8",
    );
    const negative = process.env.ZOEN_EXCALIDRAW_NEGATIVE;
    if (negative) {
      assert.ok(["mermaid-menu", "mermaid-paste"].includes(negative));
      const assets = await bundledLibraryAssets(root);
      const original = assets.plugin;
      const plugin = {
        name: "single-policy-negative",
        setup(p) {
          original.setup({
            ...p,
            onLoad(options, callback) {
              p.onLoad(options, async (args) => {
                const result = await callback(args);
                if (args.path.endsWith("/dist/prod/index.js") && result) {
                  if (negative === "mermaid-menu")
                    result.contents = result.contents.replace(
                      't.props.aiEnabled!==!1&&Me(Ce.Item,{onSelect:()=>t.setOpenDialog({name:"ttd",tab:"mermaid"})',
                      'Me(Ce.Item,{onSelect:()=>t.setOpenDialog({name:"ttd",tab:"mermaid"})',
                    );
                  else
                    result.contents = result.contents.replace(
                      "if(this.props.aiEnabled!==!1&&m.text&&lT(m.text))",
                      "if(m.text&&lT(m.text))",
                    );
                }
                return result;
              });
            },
          });
        },
      };
      const candidate = await build({
        absWorkingDir: root,
        entryPoints: ["src/mcp-entry.tsx"],
        outdir: "memory",
        bundle: true,
        write: false,
        format: "esm",
        platform: "browser",
        target: ["safari17"],
        jsx: "automatic",
        minify: true,
        conditions: ["production"],
        define: { "process.env.NODE_ENV": '"production"' },
        loader: { ".woff2": "dataurl" },
        plugins: [plugin],
      });
      html = `<!doctype html><html><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><style>${candidate.outputFiles.find((f) => f.path.endsWith(".css")).text}</style></head><body><script type="module">${candidate.outputFiles.find((f) => f.path.endsWith(".js")).text}</script></body></html>`;
    }

    const probe = `(()=>{let started=false;const errors=[];const warnings=[];const originalWarn=console.warn;console.warn=(...args)=>{warnings.push(args.map(String).join(' '));originalWarn.apply(console,args)};setTimeout(()=>{if(!started)parent.postMessage({qa:true,error:'No canvas: '+document.body.innerText.slice(0,200),errors},'*')},6000);addEventListener('error',e=>errors.push(e.message));const policy=[];addEventListener('securitypolicyviolation',e=>policy.push({directive:e.effectiveDirective,blocked:e.blockedURI}));const poll=()=>{const canvas=document.querySelector('.excalidraw canvas');if(!canvas){requestAnimationFrame(poll);return;}if(started)return;started=true;setTimeout(()=>{try{const tools=document.querySelector('[data-testid="dropdown-menu-button"][title="Mais ferramentas"]');if(!tools)throw Error('Missing real extra tools menu '+JSON.stringify([...document.querySelectorAll('button')].map(b=>[b.dataset.testid,b.title,b.getAttribute('aria-label')]).slice(0,25)));tools.click();setTimeout(()=>{try{const menu=document.querySelector('.dropdown-menu-container');if(!menu||menu.querySelectorAll('.dropdown-menu-item').length<2)throw Error('Real extra-tools menu not open');if(/mermaid/i.test(menu.textContent))throw Error('Mermaid menu remains reachable');document.dispatchEvent(new KeyboardEvent('keydown',{key:'Escape',bubbles:true}));const target=document.querySelector('.excalidraw');target.focus();const activeCanvas=document.querySelector('canvas.interactive')??document.querySelector('.excalidraw canvas');const rect=activeCanvas.getBoundingClientRect();activeCanvas.dispatchEvent(new PointerEvent('pointermove',{bubbles:true,clientX:rect.x+rect.width/2,clientY:rect.y+rect.height/2,pointerType:'mouse'}));const clipboard=new DataTransfer();clipboard.setData('text/plain','graph TD; A[Olá 世界] --> B[Texto];');const paste=new ClipboardEvent('paste',{bubbles:true,cancelable:true,clipboardData:clipboard});target.dispatchEvent(paste);setTimeout(()=>{const bounds=document.querySelector('.canonical-canvas').getBoundingClientRect();const controls=getComputedStyle(document.querySelector('.FixedSideContainer'));const box=e=>{const r=e.getBoundingClientRect();return{x:r.x,y:r.y,width:r.width,height:r.height,right:r.right,bottom:r.bottom}};const toolbar=box(document.querySelector('.App-toolbar--mobile'));const miscElement=document.querySelector('.mobile-misc-tools-container');const misc=box(miscElement);const extra=box(tools);const overlap={width:Math.max(0,Math.min(extra.right,misc.right)-Math.max(extra.x,misc.x)),height:Math.max(0,Math.min(extra.bottom,misc.bottom)-Math.max(extra.y,misc.y))};const blocked=overlap.width>0&&overlap.height>0&&document.elementsFromPoint(Math.max(extra.x,misc.x)+overlap.width/2,Math.max(extra.y,misc.y)+overlap.height/2).some(e=>miscElement.contains(e));const safeAreaGeometry={canvas:{x:bounds.x,y:bounds.y,width:bounds.width,height:bounds.height},viewport:{width:innerWidth,height:innerHeight},padding:[controls.paddingTop,controls.paddingRight,controls.paddingBottom,controls.paddingLeft],toolbar,misc,overlap,blocked};parent.postMessage({qa:true,locale:document.documentElement.lang,extraToolsLabel:tools.title,canvasCount:document.querySelectorAll('.excalidraw canvas').length,warnings,mermaidDialog:!!document.querySelector('.ttd-dialog'),errors,policy,safeAreaGeometry},'*');},1000);}catch(e){parent.postMessage({qa:true,error:String(e),errors},'*');}},100);}catch(e){parent.postMessage({qa:true,error:String(e),errors},'*');}},500);};requestAnimationFrame(poll);})();`;
    const resource = sandboxHtml(
      html.slice(0, html.lastIndexOf("</body>")) + `<script>${probe}</script></body></html>`,
      [],
    );
    const host = `import{MiniappSession}from'../../apps/zoen/src/miniapps/protocol.ts';
 const frame=document.createElement('iframe');frame.sandbox='allow-scripts';frame.style='width:420px;height:760px';
 const initial=JSON.stringify({type:'excalidraw',version:2,elements:[{id:'pixel_image',type:'image',fileId:'pixel',x:0,y:0,width:64,height:64,version:1,status:'saved'}],appState:{},files:{pixel:{id:'pixel',mimeType:'image/png',dataURL:'data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+j0qkAAAAASUVORK5CYII=',created:1}}});const bytes=new TextEncoder().encode(initial);let saved='';let upload=[];const calls=[];const statuses=[];window.qaMessages=[];
 const hex=async(bytes)=>Array.from(new Uint8Array(await crypto.subtle.digest('SHA-256',bytes)),n=>n.toString(16).padStart(2,'0')).join('');const digest=await hex(bytes);
 const session=new MiniappSession({send:m=>frame.contentWindow.postMessage(m,'*'),initialize:()=>({hostInfo:{name:'Zoen QA',version:'test'},hostCapabilities:{serverTools:{},experimental:{zoen:{canonicalScene:{version:1,itemId:'item'}}}},hostContext:{displayMode:'fullscreen',locale:'pt-BR',safeAreaInsets:{top:32,right:9,bottom:24,left:7}}}),arguments:()=>({}),state:()=>({content:[]}),context:()=>({displayMode:'fullscreen',locale:'pt-BR',safeAreaInsets:{top:32,right:9,bottom:24,left:7}}),tools:new Set(['app_scene_read','app_scene_read_chunk','app_scene_begin','app_scene_append','app_scene_commit','app_scene_abort']),resourceUri:'ui://zoen/excalidraw.html',resource:async()=>({contents:[]}),call:async(name,a)=>{calls.push(name);let value;if(name==='app_scene_read')value={transferId:'read',bytes:bytes.length,sha256:digest,chunkBytes:196608};if(name==='app_scene_read_chunk')value={index:0,data:btoa(initial)};if(name==='app_scene_begin'){upload=[];value={uploadId:'write',chunkBytes:196608};}if(name==='app_scene_append'){upload[a.index]=Uint8Array.from(atob(a.data),x=>x.charCodeAt(0));value={nextIndex:a.index+1};}if(name==='app_scene_commit'){const joined=new Uint8Array(upload.reduce((n,v)=>n+v.length,0));let at=0;for(const p of upload){joined.set(p,at);at+=p.length;}saved=new TextDecoder().decode(joined);value={checkpointId:'item',version:2,currentVersion:2,sha256:await hex(joined)};}if(name==='app_scene_abort')value={};return{content:[],structuredContent:value};},updateModelContext:async(params)=>{statuses.push(params);return{};},openLink:async()=>({}),share:async()=>({}),native:async()=>({}),haptic(){},resize(){},close(){}});
 addEventListener('message',e=>{if(e.source!==frame.contentWindow)return;window.qaMessages.push(e.data?.method??e.data?.error??'result');if(e.data?.qa){const output=document.createElement('pre');output.id='result';output.textContent=JSON.stringify({...e.data,saved,calls,statuses,messages:window.qaMessages});document.body.append(output);}else void session.receive(JSON.stringify(e.data));});frame.srcdoc=${JSON.stringify(resource)};document.body.append(frame);`;
    const built = await build({
      absWorkingDir: root,
      stdin: { contents: host, resolveDir: root, loader: "js" },
      bundle: true,
      write: false,
      format: "esm",
      platform: "browser",
      define: { "process.env.NODE_ENV": '"production"' },
    });
    const path = join(folder, "qa.html");
    await writeFile(
      path,
      `<!doctype html><body><script type="module">${built.outputFiles[0].text.replaceAll("</script", "<\\/script")}</script></body>`,
    );
    const browser = await ownedBrowserResult(chrome, folder, path);
    assert.deepEqual(browser.errors, []);
    const result = JSON.parse(browser.text);
    const evidence = process.env.ZOEN_EXCALIDRAW_EVIDENCE;
    if (evidence) {
      assert.ok(evidence.startsWith("/private/tmp/zoen-excalidraw-canonical-renderer-v89/"));
      await mkdir(evidence, { recursive: true });
      await writeFile(join(evidence, "canvas.png"), Buffer.from(browser.png, "base64"));
      await writeFile(join(evidence, "scene.json"), result.saved);
      await writeFile(
        join(evidence, "browser.json"),
        JSON.stringify({ ...result, saved: undefined, lifecycle: browser.lifecycle }, null, 2) +
          "\n",
      );
    }
    assert.equal(result.error, undefined, JSON.stringify(result));
    assert.deepEqual(result.errors, []);
    assert.deepEqual(result.warnings, []);
    assert.ok(
      result.policy.every((p) => p.directive === "script-src" && p.blocked === "eval"),
      "network or other prohibited resource attempted: " + JSON.stringify(result.policy),
    );
    assert.equal(result.locale, "pt-BR");
    assert.equal(result.extraToolsLabel, "Mais ferramentas");
    assert.ok(result.canvasCount > 0);
    assert.equal(result.mermaidDialog, false);
    assert.deepEqual(result.safeAreaGeometry.padding, ["32px", "9px", "24px", "7px"], "real pinned Excalidraw CSS keeps floating controls inside negotiated edges");
    assert.deepEqual(result.safeAreaGeometry.canvas, { x: 0, y: 0, ...result.safeAreaGeometry.viewport }, "safe controls do not inset or shrink the actual canvas");
    assert.ok(result.safeAreaGeometry.misc.y >= result.safeAreaGeometry.toolbar.bottom, "mobile Library/lock/hand controls stay below the top toolbar");
    assert.ok(result.safeAreaGeometry.misc.right <= result.safeAreaGeometry.viewport.width - 9, "mobile side tools respect the negotiated right inset");
    assert.equal(result.safeAreaGeometry.overlap.width * result.safeAreaGeometry.overlap.height, 0, "mobile side tools cannot overlap the More Tools hit area");
    assert.equal(result.safeAreaGeometry.blocked, false, "the native-safe control panel cannot intercept More Tools touches");
    assert.ok(result.saved, "Paste never reached canonical save: " + JSON.stringify(result));
    assert.ok(
      JSON.parse(result.saved).elements.some(
        (e) => e.type === "text" && e.originalText === "graph TD; A[Olá 世界] --> B[Texto];",
      ),
      "ordinary Mermaid-like Unicode paste must remain text and save through real scene callbacks: " +
        JSON.stringify(JSON.parse(result.saved).elements),
    );
    assert.ok(result.calls.includes("app_scene_commit"));
    assert.equal(
      JSON.parse(result.saved).files.pixel.dataURL,
      "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+j0qkAAAAASUVORK5CYII=",
    );
    assert.ok(
      JSON.parse(result.saved).elements.some((e) => e.type === "image" && e.fileId === "pixel"),
    );
  });
});

for (const scenario of [
  { name: "portrait", width: 420, height: 760, insets: { top: 32, right: 9, bottom: 24, left: 7 } },
  { name: "landscape", width: 760, height: 420, insets: { top: 0, right: 32, bottom: 20, left: 32 } },
  { name: "compact landscape", width: 600, height: 360, insets: { top: 0, right: 32, bottom: 20, left: 32 } },
]) {
  test(`legacy ${scenario.name} export stays clear of real toolbars and side controls`, async (t) => {
    const chrome = await findBrowserExecutable();
    if (!chrome) {
      t.skip("Chrome unavailable locally; install Chrome/Chromium or set CHROME_BIN. Native WebKit acceptance remains separate.");
      return;
    }
    const folder = await mkdtemp(join(tmpdir(), "zoen-legacy-controls-"));
    await withBrowserProfile(folder, async () => {
      const html = await readFile(join(root, "../../crates/miniapps/resources/excalidraw.html"), "utf8");
      const probe = `(() => {
        let started = false;
        const errors = [];
        addEventListener('error', event => errors.push(event.message));
        setTimeout(() => { if (!started) parent.postMessage({qa: true, error: 'No real legacy canvas', errors}, '*'); }, 6000);
        const poll = () => {
          if (!document.querySelector('.excalidraw canvas')) { requestAnimationFrame(poll); return; }
          if (started) return;
          started = true;
          setTimeout(() => {
            try {
              const box = selector => {
                const element = document.querySelector(selector);
                if (!element) throw Error('Missing real control: ' + selector);
                const r = element.getBoundingClientRect();
                return {x: r.x, y: r.y, right: r.right, bottom: r.bottom, width: r.width, height: r.height};
              };
              const exportButton = document.querySelector('button[title="Export to Excalidraw"]');
              const geometry = {
                canvas: box('.excalidraw canvas'),
                toolbar: box('.App-toolbar--mobile'),
                misc: box('.mobile-misc-tools-container'),
                bottom: box('footer.App-toolbar'),
                export: box('button[title="Export to Excalidraw"]'),
                compact: !!document.querySelector('.mobile-share-slot'),
              };
              const hit = document.elementFromPoint(geometry.export.x + geometry.export.width / 2, geometry.export.y + geometry.export.height / 2);
              parent.postMessage({qa: true, errors, geometry, exportHit: exportButton.contains(hit)}, '*');
            } catch (error) { parent.postMessage({qa: true, error: String(error), errors}, '*'); }
          }, 500);
        };
        requestAnimationFrame(poll);
      })();`;
      const resource = sandboxHtml(html.slice(0, html.lastIndexOf("</body>")) + `<script>${probe}</script></body></html>`, []);
      const host = `import {MiniappSession} from '../../apps/zoen/src/miniapps/protocol.ts';
        const frame = document.createElement('iframe');
        frame.sandbox = 'allow-scripts';
        frame.style = 'border:0;width:${scenario.width}px;height:${scenario.height}px';
        const context = {displayMode:'fullscreen',safeAreaInsets:${JSON.stringify(scenario.insets)},containerDimensions:{width:${scenario.width},height:${scenario.height}}};
        const session = new MiniappSession({
          send: message => frame.contentWindow.postMessage(message, '*'),
          initialize: () => ({hostInfo:{name:'Zoen QA',version:'controls'},hostCapabilities:{},hostContext:context}),
          context: () => context,
          arguments: () => ({elements:JSON.stringify([{id:'synthetic-rectangle',type:'rectangle',x:10,y:10,width:120,height:100,label:{text:'Synthetic layout'},version:1}])}),
          state: () => ({content:[]}), tools:new Set(), resourceUri:'ui://zoen/excalidraw.html',
          resource:async()=>({contents:[]}), call:async()=>({content:[]}), updateModelContext:async()=>({}),
          openLink:async()=>({}), share:async()=>({}), native:async()=>({}), haptic(){}, resize(){}, close(){},
        });
        addEventListener('message', event => {
          if (event.source !== frame.contentWindow) return;
          if (event.data?.qa) {
            const result = document.createElement('pre'); result.id = 'result';
            result.textContent = JSON.stringify(event.data); document.body.append(result);
          } else void session.receive(JSON.stringify(event.data));
        });
        frame.srcdoc = ${JSON.stringify(resource)}; document.body.append(frame);`;
      const built = await build({absWorkingDir: root, stdin: {contents: host, resolveDir: root, loader: "js"}, bundle: true, write: false, format: "esm", platform: "browser", define: {"process.env.NODE_ENV": '"production"'}});
      const path = join(folder, "qa.html");
      await writeFile(path, `<!doctype html><body style="margin:0"><script type="module" src="data:text/javascript;base64,${Buffer.from(built.outputFiles[0].text).toString("base64")}"></script></body>`);
      const browser = await ownedBrowserResult(chrome, folder, path);
      assert.deepEqual(browser.errors, []);
      const result = JSON.parse(browser.text);
      assert.equal(result.error, undefined, JSON.stringify(result));
      assert.deepEqual(result.errors, []);
      const {geometry} = result;
      const overlap = (a, b) => Math.max(0, Math.min(a.right, b.right) - Math.max(a.x, b.x)) * Math.max(0, Math.min(a.bottom, b.bottom) - Math.max(a.y, b.y));
      for (const control of [geometry.toolbar, geometry.misc, geometry.bottom])
        assert.equal(overlap(geometry.export, control), 0, `Export overlaps another control: ${JSON.stringify(geometry)}`);
      assert.equal(overlap(geometry.misc, geometry.toolbar), 0);
      assert.equal(overlap(geometry.misc, geometry.bottom), 0);
      assert.ok(geometry.export.x >= scenario.insets.left);
      assert.ok(geometry.export.right <= scenario.width - scenario.insets.right);
      assert.ok(geometry.export.y >= scenario.insets.top);
      assert.ok(geometry.export.bottom <= scenario.height - scenario.insets.bottom);
      assert.ok(geometry.misc.right <= scenario.width - scenario.insets.right);
      assert.equal(result.exportHit, true, "Export's center remains an unobstructed real hit target");
      assert.equal(geometry.compact, scenario.width <= 640);
      assert.deepEqual(geometry.canvas, {x:0,y:0,width:scenario.width,height:scenario.height,right:scenario.width,bottom:scenario.height});
    });
  });
}
