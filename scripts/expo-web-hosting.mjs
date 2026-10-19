#!/usr/bin/env node
// Actual Expo export in Pages or Workers. HTTP only; no browser or relay required.
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { createServer } from "node:net";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { setTimeout as delay } from "node:timers/promises";
import { inspectExpoWebArtifact } from "./expo-web-artifact.mjs";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
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
export async function verifyExpoWebHosting(origin, directory, runtime = "pages") {
  const index = await readFile(join(directory, "index.html"), "utf8");
  const results = [];
  for (const path of [
    "/",
    "/chat/sp_export_acceptance_unknown",
    "/sign-in",
    "/sign-in?acceptance=1",
    "/sign-in/",
    "/sign-in.html",
    "/sign-in-extra",
    "/sign-in/child",
    "/worker.js",
    "/core/zoen_web_core_bg.wasm",
  ]) {
    const response = await fetch(origin + path, {
      redirect: "manual",
      signal: AbortSignal.timeout(5000),
      // Workers' SPA fallback intentionally applies only to document navigation.
      // Asset fetches retain their ordinary request mode and MIME assertions.
      ...(runtime === "workers" && !/\.(js|wasm)$/.test(path)
        ? { headers: { "Sec-Fetch-Mode": "navigate", Accept: "text/html" } }
        : {}),
    });
    const auth = /^\/sign-in(?:\?|\/$|\.html$|$)/.test(path);
    const redirect = path === "/sign-in/" || path === "/sign-in.html";
    assert.equal(response.status, redirect ? 308 : 200, `${path}: status`);
    assert.equal(
      response.headers.get("Cross-Origin-Opener-Policy"),
      auth ? "same-origin-allow-popups" : "same-origin",
      `${path}: COOP`,
    );
    assert.equal(
      response.headers.get("Cross-Origin-Embedder-Policy"),
      auth ? "unsafe-none" : "require-corp",
      `${path}: COEP`,
    );
    const type = response.headers.get("content-type");
    if (redirect)
      assert.equal(response.headers.get("location"), "/sign-in", `${path}: canonical destination`);
    else if (path.endsWith(".wasm"))
      assert.match(type ?? "", /^application\/wasm(?:;|$)/, `${path}: WASM MIME`);
    else if (path.endsWith(".js"))
      assert.match(type ?? "", /(?:application|text)\/javascript/, `${path}: worker MIME`);
    else {
      assert.match(type ?? "", /^text\/html(?:;|$)/, `${path}: HTML MIME`);
      assert.equal(await response.text(), index, `${path}: must serve the actual SPA entry`);
    }
    await response.body?.cancel().catch(() => {});
    results.push({ path, status: response.status, type });
  }
  return results;
}
if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const directory = resolve(
    process.argv[2] ?? process.env.ZOEN_EXPO_DIST ?? join(root, "apps/zoen/dist"),
  );
  const artifact = await inspectExpoWebArtifact(directory);
  const runtime = process.argv.includes("--workers") ? "workers" : "pages";
  const work = await mkdtemp(join(tmpdir(), "zoen-expo-hosting-"));
  let child, done;
  try {
    const [port, inspector] = await Promise.all([freePort(), freePort()]);
    const tools = resolve(process.env.ZOEN_E2E_TOOLS ?? join(root, "scripts/expo-web-tests"));
    const cli = process.env.WRANGLER_CLI ?? join(tools, "node_modules/wrangler/bin/wrangler.js");
    const config = join(work, "wrangler.json");
    if (runtime === "workers")
      await writeFile(
        config,
        JSON.stringify({
          name: "zoen-web-hosting-qa",
          compatibility_date: "2026-10-18",
          assets: { directory, not_found_handling: "single-page-application" },
        }),
      );
    let output = "";
    child = spawn(
      process.execPath,
      [
        cli,
        ...(runtime === "workers" ? ["dev", "--config", config] : ["pages", "dev", directory]),
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
        join(work, "state"),
      ],
      {
        cwd: work,
        env: {
          ...process.env,
          WRANGLER_SEND_METRICS: "false",
          WRANGLER_LOG_PATH: join(work, "wrangler.log"),
        },
        detached: process.platform !== "win32",
        stdio: ["ignore", "pipe", "pipe"],
      },
    );
    child.stdout.on("data", (chunk) => {
      output = (output + chunk).slice(-16000);
    });
    child.stderr.on("data", (chunk) => {
      output = (output + chunk).slice(-16000);
    });
    let spawnError;
    done = new Promise((accept) => {
      child.once("error", (error) => {
        spawnError = error;
        accept();
      });
      child.once("exit", accept);
    });
    const origin = `http://127.0.0.1:${port}`;
    let ready = false;
    for (let attempt = 0; attempt < 200; attempt++) {
      if (spawnError) throw spawnError;
      if (child.exitCode !== null) throw new Error(`${runtime} exited before readiness: ${output}`);
      try {
        const response = await fetch(origin, { signal: AbortSignal.timeout(500) });
        await response.body?.cancel();
        if (response.ok) {
          ready = true;
          break;
        }
      } catch {
        /* Wait for this owned local Cloudflare process. */
      }
      await delay(100);
    }
    assert.ok(ready, `${runtime} did not become ready: ${output}`);
    const paths = await verifyExpoWebHosting(origin, directory, runtime);
    console.log(JSON.stringify({ passed: true, runtime, artifact, paths }, null, 2));
  } finally {
    if (child?.pid && child.exitCode === null && child.signalCode === null) {
      const signal = (value) => {
        try {
          if (process.platform === "win32") child.kill(value);
          else process.kill(-child.pid, value);
        } catch (error) {
          if (error.code !== "ESRCH") throw error;
        }
      };
      signal("SIGINT");
      if (!(await Promise.race([done.then(() => true), delay(3000, false)]))) {
        signal("SIGKILL");
        await done;
      }
    }
    await rm(work, { recursive: true, force: true });
  }
}
