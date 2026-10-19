#!/usr/bin/env node
// Executes Cloudflare's actual Pages header engine, including detachment and redirects.
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { existsSync } from "node:fs";
import { cp, mkdir, mkdtemp, rm, writeFile } from "node:fs/promises";
import { createServer } from "node:net";
import { delimiter, dirname, join, resolve } from "node:path";
import { tmpdir } from "node:os";
import { fileURLToPath } from "node:url";
import { setTimeout as delay } from "node:timers/promises";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const cli = [
  process.env.WRANGLER_CLI,
  join(root, "apps/zoen/node_modules/.bin/wrangler"),
  ...(process.env.PATH ?? "").split(delimiter).map((folder) => join(folder, "wrangler")),
].find((file) => file && existsSync(file));
if (!cli) throw new Error("Run with Wrangler on PATH, or set WRANGLER_CLI to its executable.");

async function freePort() {
  const server = createServer();
  await new Promise((accept, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", accept);
  });
  const port = server.address().port;
  await new Promise((accept, reject) =>
    server.close((error) => (error ? reject(error) : accept())),
  );
  return port;
}
const fixture = await mkdtemp(join(tmpdir(), "zoen-pages-headers-"));
let child;
let exited;
try {
  const site = join(fixture, "site");
  await mkdir(join(site, "core"), { recursive: true });
  await cp(join(root, "apps/zoen/public/_headers"), join(site, "_headers"));
  await writeFile(join(site, "index.html"), "<!doctype html><title>App</title>Isolated app");
  await writeFile(join(site, "sign-in.html"), "<!doctype html><title>Sign in</title>OAuth entry");
  await writeFile(
    join(site, "sign-in-extra.html"),
    "<!doctype html><title>App</title>Isolated app",
  );
  await writeFile(join(site, "404.html"), "<!doctype html><title>Missing</title>Missing");
  await writeFile(join(site, "core/worker.js"), "self.onmessage = () => {};");
  const [port, inspector] = await Promise.all([freePort(), freePort()]);
  let output = "";
  child = spawn(
    process.execPath,
    [
      cli,
      "pages",
      "dev",
      site,
      "--ip",
      "127.0.0.1",
      "--port",
      String(port),
      "--inspector-port",
      String(inspector),
      "--compatibility-date",
      "2026-10-17",
      "--no-show-interactive-dev-session",
      "--persist-to",
      join(fixture, "state"),
    ],
    {
      cwd: fixture,
      env: {
        ...process.env,
        WRANGLER_SEND_METRICS: "false",
        WRANGLER_LOG_PATH: join(fixture, "wrangler.log"),
      },
      stdio: ["ignore", "pipe", "pipe"],
    },
  );
  const append = (chunk) => {
    output = `${output}${chunk}`.slice(-16_000);
  };
  child.stdout.on("data", append);
  child.stderr.on("data", append);
  exited = new Promise((accept) => child.once("exit", (code) => accept(code)));
  let ready = false;
  for (let attempt = 0; attempt < 200; attempt++) {
    if (child.exitCode !== null)
      throw new Error(`Wrangler exited before serving the fixture.\n${output}`);
    try {
      const response = await fetch(`http://127.0.0.1:${port}/`, {
        signal: AbortSignal.timeout(500),
      });
      await response.arrayBuffer();
      ready = true;
      break;
    } catch {
      await delay(100);
    }
  }
  assert.ok(ready, `Wrangler did not start.\n${output}`);
  const entry = new Set(["/sign-in", "/sign-in/", "/sign-in.html", "/sign-in?probe=1"]);
  const paths = [...entry, "/", "/sign-in-extra", "/sign-in/child", "/core/worker.js", "/missing"];
  for (const path of paths) {
    const response = await fetch(`http://127.0.0.1:${port}${path}`, { redirect: "manual" });
    const auth = entry.has(path);
    // Pages' generated missing-page response owns its no-store policy; app
    // documents and assets must retain our exact no-transform policy.
    if (response.status === 200)
      assert.match(
        response.headers.get("Cache-Control") ?? "",
        /(?:^|,)\s*no-transform\s*(?:,|$)/i,
        `${path}: no automatic edge payload transformations`,
      );
    assert.equal(
      response.headers.get("Cross-Origin-Opener-Policy"),
      auth ? "same-origin-allow-popups" : "same-origin",
      `${path}: exact COOP, never comma-joined`,
    );
    assert.equal(
      response.headers.get("Cross-Origin-Embedder-Policy"),
      auth ? "unsafe-none" : "require-corp",
      `${path}: exact COEP`,
    );
    if (path === "/sign-in/" || path === "/sign-in.html") {
      assert.equal(response.status, 308);
      assert.equal(response.headers.get("Location"), "/sign-in");
    } else if (path === "/sign-in" || path.startsWith("/sign-in?")) {
      assert.equal(response.status, 200);
      assert.ok((await response.text()).includes("OAuth entry"));
    }
    await response.arrayBuffer().catch(() => {});
  }
  console.log(
    `Cloudflare Pages headers verified on ${paths.length} paths, including exact OAuth aliases, redirects, worker assets and 404s.`,
  );
} finally {
  if (child && child.exitCode === null) {
    child.kill("SIGINT");
    if ((await Promise.race([exited.then(() => true), delay(3000, false)])) === false) {
      child.kill("SIGKILL");
      await exited;
    }
  }
  await rm(fixture, { recursive: true, force: true });
}
