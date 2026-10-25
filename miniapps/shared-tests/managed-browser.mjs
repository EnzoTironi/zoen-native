//! Actual generated MCP bootstrap and engine, with a synthetic canonical read/CAS port.
// This helper never claims Rust persistence, agent authorization or iPhone acceptance.
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { createServer } from "node:http";
import { readFile, mkdir, writeFile } from "node:fs/promises";
import { resolve, isAbsolute, relative } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { build } from "../../apps/zoen/node_modules/esbuild/lib/main.js";
import { findBrowserExecutable } from "../../tools/miniapp-build/browser-executable.ts";
import { binaryEditorFormat } from "../../apps/zoen/src/miniapps/binary-formats.ts";
export const sha = (bytes) => createHash("sha256").update(bytes).digest("hex");
const root = resolve(fileURLToPath(new URL("../..", import.meta.url)));
export async function openManagedBrowser(options) {
  const { editor, modulePath, wasmPath, htmlPath, seedPath, output, playwrightPath } = options;
  assert.ok([modulePath, wasmPath, htmlPath, seedPath, output, playwrightPath].every(isAbsolute));
  const outside = relative(root, output);
  assert.ok(outside === ".." || outside.startsWith("../"), "QA output must remain outside source");
  await mkdir(output);
  const format = binaryEditorFormat(editor);
  const module = await readFile(modulePath),
    wasm = await readFile(wasmPath);
  const html = await readFile(htmlPath, "utf8"),
    seed = await readFile(seedPath);
  assert.ok(module.length > 0 && module.length <= 512 * 1024);
  assert.ok(wasm.length > 8 && wasm.length <= 48 * 1024 * 1024);
  assert.ok(html.includes(`${editor}_canvas`) && html.includes('name="viewport"'));
  const asset = (bytes, suffix, mime) => ({
    url: `https://code.example.test/${editor}/${format.engineRevision}/${sha(bytes)}.${suffix}`,
    bytes: bytes.length,
    sha256: sha(bytes),
    mime,
  });
  let config = {
    editor,
    html,
    seed: seed.toString("base64"),
    host: options.host,
    runtime: {
      schema: 1,
      engine: editor,
      revision: format.engineRevision,
      abi: "zoen-host-v1",
      module: asset(module, "js", "text/javascript"),
      wasm: asset(wasm, "wasm", "application/wasm"),
    },
  };
  const bundled = await build({
    absWorkingDir: root,
    entryPoints: ["miniapps/gridcraft-ui/test/managed-bootstrap.fixture.ts"],
    bundle: true,
    write: false,
    format: "esm",
    platform: "browser",
    target: ["safari17"],
    preserveSymlinks: true,
    nodePaths: [resolve(root, "apps/zoen/node_modules")],
    define: { "process.env.NODE_ENV": '"production"', "process.env.EXPO_OS": '"web"' },
  });
  const files = new Map([
    ["/module.js", [module, "text/javascript"]],
    ["/engine.wasm", [wasm, "application/wasm"]],
    ["/probe.js", [bundled.outputFiles[0].contents, "text/javascript"]],
    ["/favicon.ico", [Buffer.alloc(0), "image/x-icon"]],
    [
      "/",
      [
        Buffer.from(
          '<!doctype html><meta name="viewport" content="width=device-width,initial-scale=1"><style>html,body{margin:0}iframe{width:100vw;height:100vh;border:0}</style><iframe sandbox="allow-scripts"></iframe><script type="module">import {startProbe} from "/probe.js";await startProbe(await(await fetch("/config.json")).json());</script>',
        ),
        "text/html",
      ],
    ],
  ]);
  const requests = [],
    errors = [],
    consoleErrors = [],
    cleanupErrors = [];
  const server = createServer((request, response) => {
    requests.push({ method: request.method, path: request.url });
    const file =
      request.url === "/config.json"
        ? [Buffer.from(JSON.stringify(config)), "application/json"]
        : files.get(request.url);
    if (request.method !== "GET" || !file) {
      response.writeHead(404);
      response.end();
      return;
    }
    response.writeHead(200, {
      "Content-Type": file[1],
      "Content-Length": file[0].length,
      "Cache-Control": "no-store",
    });
    response.end(file[0]);
  });
  let browser;
  const inputs = {
    scope:
      "Actual generated MCP bootstrap/engine and shared session/export; synthetic Core read/CAS port only.",
    editor,
    revision: format.engineRevision,
    html: { bytes: Buffer.byteLength(html), sha256: sha(html) },
    module: { bytes: module.length, sha256: sha(module) },
    wasm: { bytes: wasm.length, sha256: sha(wasm) },
    source: { bytes: seed.length, sha256: sha(seed) },
    fixtureBundleSha256: sha(bundled.outputFiles[0].contents),
    host: options.host ?? null,
    deviceScaleFactor: options.deviceScaleFactor ?? 1,
  };
  async function close() {
    try {
      await browser?.close();
    } catch (error) {
      cleanupErrors.push(String(error));
    }
    server.closeAllConnections();
    if (server.listening) await new Promise((done) => server.close(done));
  }
  try {
    await new Promise((done, fail) => {
      server.once("error", fail);
      server.listen(0, "127.0.0.1", done);
    });
    const { chromium, webkit, devices } = await import(pathToFileURL(playwrightPath).href);
    const isWebkit = process.env.ZOEN_PROBE_WEBKIT === "1";
    browser = await (isWebkit ? webkit : chromium).launch(
      isWebkit
        ? { headless: true, timeout: 30000 }
        : {
            executablePath: await findBrowserExecutable(),
            headless: true,
            timeout: 30000,
            args: [
              "--enable-unsafe-swiftshader",
              "--use-angle=swiftshader",
              "--disable-background-networking",
              "--disable-component-update",
            ],
          },
    );
    const page = await browser.newPage({
      ...(isWebkit ? devices["iPhone 13"] : {}),
      viewport: { width: 402, height: 874 },
      deviceScaleFactor: options.deviceScaleFactor ?? 1,
      hasTouch: true,
    });
    page.on("pageerror", (error) => errors.push(String(error)));
    page.on("console", (message) => {
      if (message.type() === "error") consoleErrors.push(message.text());
    });
    const url = `http://127.0.0.1:${server.address().port}/`;
    const opened = async () =>
      page.waitForFunction(() => window.probe?.opened, undefined, { timeout: 30000 });
    await page.goto(url, { waitUntil: "load" });
    await opened();
    await writeFile(resolve(output, "inputs.json"), JSON.stringify(inputs, null, 2) + "\n");
    return {
      page,
      inputs,
      seed,
      requests,
      errors,
      consoleErrors,
      cleanupErrors,
      close,
      async reopen(accepted, version) {
        config = { ...config, seed: accepted, version };
        await page.reload({ waitUntil: "load" });
        await opened();
      },
    };
  } catch (error) {
    await close();
    await writeFile(
      resolve(output, "startup-failure.json"),
      JSON.stringify(
        { error: String(error), inputs, requests, errors, consoleErrors, cleanupErrors },
        null,
        2,
      ) + "\n",
    );
    throw error;
  }
}
