#!/usr/bin/env node
// Native/WASM import and verification smoke; no relay, credentials or deployment.
import assert from "node:assert/strict";
import { execFileSync, spawn } from "node:child_process";
import { createHash } from "node:crypto";
import { access, mkdir, mkdtemp, readFile, writeFile } from "node:fs/promises";
import { constants } from "node:fs";
import { createRequire } from "node:module";
import { tmpdir } from "node:os";
import { delimiter, dirname, join, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const runnerPath = fileURLToPath(import.meta.url);
const sourceScope = [
  "Cargo.toml", "Cargo.lock", "crates/relay-admission", "crates/event-log",
  "crates/protocol", "crates/mls", "crates/types", "crates/zoen-relay",
  "scripts/verify-cloudflare-admission.mjs",
];
let requestedOutput;
let skipBuild = false;
const args = process.argv.slice(2);
for (let index = 0; index < args.length; index++) {
  if (args[index] === "--skip-build") skipBuild = true;
  else if (args[index] === "--out-dir" && args[index + 1] && !args[index + 1].startsWith("--"))
    requestedOutput = args[++index];
  else throw new Error("Usage: verify-cloudflare-admission.mjs [--out-dir DIR] [--skip-build]");
}
const output = requestedOutput
  ? resolve(requestedOutput)
  : await mkdtemp(join(tmpdir(), "zoen-cloudflare-admission-"));
const wasmDirectory = join(output, "wasm");
const fixturesPath = join(output, "native-fixtures.json");
const modulePath = join(wasmDirectory, "admission.js");
const wasmPath = join(wasmDirectory, "admission_bg.wasm");
const scope = "Import/verification smoke only: shared Rust native fixtures versus Node and local workerd. No public authentication, storage adapter, encrypted sync or deployment acceptance.";
const checks = [
  "native-canonical-bytes-and-effect",
  "verified-session-keys",
  "signature-rejection",
  "exact-u64",
];
await mkdir(wasmDirectory, { recursive: true });
console.log(`Admission smoke output: ${output}`);

function run(command, commandArgs, capture = false, environment = process.env) {
  console.log(`Running ${command} ${commandArgs.join(" ")}`);
  return new Promise((accept, reject) => {
    const child = spawn(command, commandArgs, {
      cwd: root,
      env: environment,
      stdio: ["ignore", capture ? "pipe" : "inherit", "inherit"],
    });
    const chunks = [];
    if (capture) child.stdout.on("data", (chunk) => chunks.push(chunk));
    child.once("error", reject);
    child.once("close", (code, signal) => {
      if (code !== 0) reject(new Error(`${command} failed (${signal ?? code}).`));
      else accept(capture ? Buffer.concat(chunks).toString("utf8") : undefined);
    });
  });
}

async function wasmEnvironment() {
  if (process.env.AR_wasm32_unknown_unknown || process.env["AR_wasm32-unknown-unknown"])
    return process.env;
  // Host Apple/GNU ar may accept WASM objects without indexing their symbols.
  // Prefer LLVM's archiver for ring's C objects, without changing native builds.
  const suffix = process.platform === "win32" ? ".exe" : "";
  const candidates = (process.env.PATH ?? "").split(delimiter)
    .filter(Boolean).map(directory => join(directory, `llvm-ar${suffix}`));
  const sysroot = execFileSync("rustc", ["--print", "sysroot"], { encoding: "utf8" }).trim();
  const host = /^host: (.+)$/m.exec(execFileSync("rustc", ["-vV"], { encoding: "utf8" }))?.[1];
  if (host) candidates.push(join(sysroot, "lib", "rustlib", host, "bin", `llvm-ar${suffix}`));
  for (const path of candidates) {
    try {
      await access(path, constants.X_OK);
      return { ...process.env, AR_wasm32_unknown_unknown: path };
    } catch (error) {
      if (!["ENOENT", "EACCES", "ENOTDIR"].includes(error.code)) throw error;
    }
  }
  throw new Error("WASM RSA requires llvm-ar. Install rustup component add llvm-tools-preview, or set AR_wasm32_unknown_unknown to LLVM ar.");
}

async function save(name, value) {
  await writeFile(join(output, name), `${JSON.stringify(value, null, 2)}\n`);
}

async function provenance() {
  const git = (...gitArgs) => execFileSync("git", gitArgs, {
    cwd: root, encoding: "utf8", stdio: ["ignore", "pipe", "pipe"],
  }).trim();
  const head = git("rev-parse", "HEAD");
  const artifacts = {};
  for (const [name, path] of [
    ["nativeFixtures", fixturesPath], ["javascript", modulePath],
    ["wasm", wasmPath], ["runner", runnerPath],
  ]) {
    try {
      const bytes = await readFile(path);
      artifacts[name] = { path, bytes: bytes.length, sha256: createHash("sha256").update(bytes).digest("hex") };
    } catch (error) {
      if (error.code !== "ENOENT") throw error;
      artifacts[name] = { path, missing: true };
    }
  }
  return {
    source: {
      commit: process.env.GITHUB_SHA || head,
      commitSource: process.env.GITHUB_SHA ? "GITHUB_SHA" : "git rev-parse HEAD",
      checkoutHead: head,
      dirty: git("status", "--porcelain", "--untracked-files=all", "--", ...sourceScope).length > 0,
      dirtyScope: sourceScope,
    },
    artifacts,
  };
}

function signatureFailure(operation) {
  let failure;
  assert.throws(operation, (value) => {
    failure = JSON.parse(String(value));
    return failure.code === "bad_signature" && failure.permanent === true;
  });
  return failure;
}

function checkResult(result, fixture) {
  assert.deepEqual(result.accepted, fixture.expected);
  for (const key of ["space_id", "author", "client_id"])
    assert.equal(result.verified[key], fixture.expected[key]);
  assert.equal(result.signatureError?.code, "bad_signature");
  assert.equal(result.signatureError.permanent, true);
}

const workerSource = `import { initSync, admitPublish, verifyPublish } from './admission.js';
import wasmModule from './admission_bg.wasm';
initSync({ module: wasmModule });
function decode(value) { return Uint8Array.from(atob(value), char => char.charCodeAt(0)); }
export default {
  async fetch(request) {
    const fixture = await request.json();
    const frame = decode(fixture.frame_base64);
    let signatureError = null;
    try { admitPublish(decode(fixture.invalid_signature_frame_base64), fixture.context); }
    catch (error) { signatureError = JSON.parse(String(error)); }
    return Response.json({
      accepted: JSON.parse(admitPublish(frame, fixture.context)),
      verified: JSON.parse(verifyPublish(frame, JSON.stringify(fixture.session))),
      signatureError,
    });
  },
};
`;

try {
  if (!skipBuild) {
    const generated = await run("cargo", [
      "run", "--quiet", "--locked", "-p", "relay-admission", "--example", "portable-fixture",
    ], true);
    JSON.parse(generated);
    await writeFile(fixturesPath, generated);
    await run("cargo", [
      "build", "--locked", "--release", "-p", "relay-admission", "--target", "wasm32-unknown-unknown",
    ], false, await wasmEnvironment());
    const target = process.env.CARGO_TARGET_DIR
      ? resolve(root, process.env.CARGO_TARGET_DIR)
      : join(root, "target");
    await run("wasm-bindgen", [
      join(target, "wasm32-unknown-unknown/release/relay_admission.wasm"),
      "--target", "web", "--out-name", "admission", "--out-dir", wasmDirectory,
    ]);
  }
  await writeFile(join(wasmDirectory, "package.json"), '{"type":"module"}\n');
  const { fixtures } = JSON.parse(await readFile(fixturesPath, "utf8"));
  assert.ok(Array.isArray(fixtures), "Native fixtures must be an array.");
  assert.equal(fixtures.length, 2, "Both Rust fixtures must run.");
  assert.equal(fixtures[0].expected.event.seq, "0");
  assert.equal(fixtures[1].expected.event.seq, "9007199254740994");

  const { initSync, admitPublish, verifyPublish } = await import(pathToFileURL(modulePath).href);
  initSync({ module: await readFile(wasmPath) });
  const nodeResults = [];
  for (const fixture of fixtures) {
    const frame = Uint8Array.from(Buffer.from(fixture.frame_base64, "base64"));
    const result = {
      accepted: JSON.parse(admitPublish(frame, fixture.context)),
      verified: JSON.parse(verifyPublish(frame, JSON.stringify(fixture.session))),
      signatureError: signatureFailure(() => admitPublish(
        Uint8Array.from(Buffer.from(fixture.invalid_signature_frame_base64, "base64")),
        fixture.context,
      )),
    };
    checkResult(result, fixture);
    nodeResults.push(result);
  }
  await save("node-result.json", {
    passed: true, runtime: "node", positiveFixtures: fixtures.length, checks, results: nodeResults, scope,
  });
  console.log("PASS Node: exact native results, session keys, signatures and UInt64.");

  const requireTooling = createRequire(join(root, "infra/cloudflare/package.json"));
  const { Miniflare, convertV4MiniflareOptions } = requireTooling("miniflare");
  const entry = join(wasmDirectory, "workerd-entry.mjs");
  await writeFile(entry, workerSource);
  const runtime = new Miniflare(convertV4MiniflareOptions({
    modules: [
      { type: "ESModule", path: entry },
      { type: "ESModule", path: modulePath },
      { type: "CompiledWasm", path: wasmPath },
    ],
    modulesRoot: wasmDirectory,
    compatibilityDate: "2026-10-18",
  }));
  const workerdResults = [];
  try {
    for (const fixture of fixtures) {
      const response = await runtime.dispatchFetch("http://admission-import.test", {
        method: "POST", body: JSON.stringify(fixture),
      });
      const body = await response.text();
      assert.equal(response.status, 200, body);
      const result = JSON.parse(body);
      checkResult(result, fixture);
      workerdResults.push(result);
    }
    await save("workerd-result.json", {
      passed: true, runtime: "workerd", positiveFixtures: fixtures.length,
      checks: ["precompiled-wasm-import", ...checks], results: workerdResults, scope,
    });
  } finally {
    await runtime.dispose();
  }
  const summary = {
    passed: true, output, skipBuild, positiveFixturesPerRuntime: fixtures.length,
    runtimes: ["node", "workerd"], checks: ["precompiled-wasm-import", ...checks],
    provenance: await provenance(), scope,
  };
  await save("summary.json", summary);
  console.log(JSON.stringify(summary));
} catch (error) {
  await save("summary.json", {
    passed: false, output, skipBuild, error: error instanceof Error ? error.message : String(error),
    provenance: await provenance(), scope,
  });
  throw error;
}
