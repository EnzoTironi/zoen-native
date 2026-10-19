#!/usr/bin/env node
// Existing native RS256 policy versus its actual Node/workerd WASM execution.
import assert from "node:assert/strict";
import { execFileSync, spawn } from "node:child_process";
import { createHash } from "node:crypto";
import { copyFile, mkdir, mkdtemp, readFile, writeFile } from "node:fs/promises";
import { createRequire } from "node:module";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
let output,
  skipBuild = false,
  host = false;
const args = process.argv.slice(2);
for (let index = 0; index < args.length; index++) {
  if (args[index] === "--skip-build") skipBuild = true;
  else if (args[index] === "--host") host = true;
  else if (args[index] === "--out-dir" && args[index + 1] && !args[index + 1].startsWith("--"))
    output = resolve(args[++index]);
  else
    throw new Error(
      "Usage: verify-cloudflare-provider.mjs [--out-dir DIR] [--skip-build] [--host]",
    );
}
output ??= await mkdtemp(join(tmpdir(), "zoen-cloudflare-provider-"));
await mkdir(join(output, "wasm"), { recursive: true });
const scope = host
  ? "Private provider host: actual isolated native PostgreSQL issue/bind oracle versus shared Rust/WASM+SQLite, plus bounded host I/O and HTTP-origin checks. Outer Worker stays404; no public provider confirmation or deployment acceptance."
  : "Synthetic native/Node/workerd provider assertion equivalence only; no public HTTP auth, challenge/proof consumption, account binding, provider confirmation or deployment acceptance.";
const sourceScopes = [
  ".github/workflows/expo.yml",
  "infra/cloudflare/package.json",
  "infra/cloudflare/package-lock.json",
  "Cargo.toml",
  "Cargo.lock",
  "crates/relay-admission",
  "crates/protocol",
  "crates/mls",
  "crates/event-log",
  "crates/types",
  "crates/zoen-relay/src/auth.rs",
  "crates/zoen-relay/src/auth",
  "crates/zoen-relay/migrations/0033_android_provider_client.sql",
  "scripts/verify-cloudflare-provider.mjs",
  "scripts/verify-cloudflare-admission.mjs",
  "scripts/build-cloudflare-closed.mjs",
  ...(host
    ? [
        "infra/cloudflare/relay-state/src/directory-state.mjs",
        "infra/cloudflare/relay-state/src/directory-index.mjs",
        "infra/cloudflare/relay-state/src/relay-entrypoint.mjs",
        "infra/cloudflare/relay-state/src/relay-route.mjs",
        "infra/cloudflare/relay-state/src/relay-composition.mjs",
  "infra/cloudflare/relay-state/src/relay-presence.mjs",
  "infra/cloudflare/relay-state/src/socket-transport.mjs",
  "infra/cloudflare/relay-state/src/socket-alarm.mjs",
        "infra/cloudflare/relay-state/src/provider-auth.mjs",
        "infra/cloudflare/relay-state/src/media-blobs.mjs",
        "infra/cloudflare/relay-state/src/blob-io.mjs",
        "infra/cloudflare/relay-state/src/blob-quota.mjs",
        "infra/cloudflare/relay-state/src/transfer-blobs.mjs",
        "infra/cloudflare/relay-state/package.json",
        "infra/cloudflare/relay-state/wrangler.directory.jsonc",
        "infra/cloudflare/relay-state/tests/directory-state.test.mjs",
        "infra/cloudflare/relay-state/tests/directory-socket.test.mjs",
        "infra/cloudflare/relay-state/tests/socket-lifecycle-harness.mjs",
        "infra/cloudflare/relay-state/tests/socket-lifecycle-child.mjs",
        "infra/cloudflare/relay-state/tests/socket-lifecycle-process.mjs",
        "infra/cloudflare/relay-state/tests/directory-lifecycle-fixture.mjs",
        "infra/cloudflare/relay-state/tests/provider-fixture.mjs",
        "infra/cloudflare/relay-state/tests/provider-state.test.mjs",
      ]
    : []),
];
const git = (...a) => execFileSync("git", a, { cwd: root, encoding: "utf8" }).trim();
function run(command, commandArgs, capture = false, environment = process.env) {
  return new Promise((accept, reject) => {
    const child = spawn(command, commandArgs, {
      cwd: root,
      env: environment,
      stdio: ["ignore", capture ? "pipe" : "inherit", "inherit"],
    });
    const chunks = [];
    if (capture) child.stdout.on("data", (chunk) => chunks.push(chunk));
    child.once("error", reject);
    child.once("close", (code, signal) =>
      code === 0
        ? accept(capture ? Buffer.concat(chunks).toString("utf8") : undefined)
        : reject(
            Object.assign(new Error(`${command} failed (${signal ?? code})`), {
              stdout: capture ? Buffer.concat(chunks).toString("utf8") : undefined,
            }),
          ),
    );
  });
}
async function save(name, value) {
  await writeFile(join(output, name), `${JSON.stringify(value, null, 2)}\n`);
}
async function artifact(path) {
  const bytes = await readFile(path);
  return { path, bytes: bytes.length, sha256: createHash("sha256").update(bytes).digest("hex") };
}
const sourcePaths = git(
  "ls-files",
  "--cached",
  "--others",
  "--exclude-standard",
  "--",
  ...sourceScopes,
)
  .split("\n")
  .filter(Boolean);
// Deleted fixture paths are absent after the authoritative move.
const sources = [];
for (const path of [...new Set(sourcePaths)].sort()) {
  try {
    sources.push(await artifact(join(root, path)));
  } catch (error) {
    if (error.code !== "ENOENT") throw error;
  }
}
const entrySource = `import {initSync,verifyProviderAssertion,providerKeyId,providerKeySet} from './admission.js';
import wasmModule from './admission_bg.wasm';
initSync({module:wasmModule});
export default{async fetch(request){const f=await request.json();let verified=null,error=null;
try{verified=JSON.parse(verifyProviderAssertion(f.token,JSON.stringify(f.key),JSON.stringify(f.context),f.now_ms));}
catch(failure){error=String(failure);}
return Response.json({verified,error});}};`;
try {
  const fixture = JSON.parse(
    await run(
      "cargo",
      [
        "run",
        "--quiet",
        "--locked",
        "-p",
        "relay-admission",
        "--example",
        "provider-assertion-fixture",
      ],
      true,
    ),
  );
  assert.equal(fixture.schema, 1);
  assert.equal(fixture.cases.length, 18);
  await save("native-fixtures.json", fixture);
  if (!skipBuild) await run(process.execPath, [join(root, "scripts/build-cloudflare-closed.mjs")]);
  for (const name of ["admission.js", "admission_bg.wasm"])
    await copyFile(
      join(root, "infra/cloudflare/relay-state/generated/wasm", name),
      join(output, "wasm", name),
    );
  await writeFile(join(output, "wasm/package.json"), '{"type":"module"}\n');
  const modulePath = join(output, "wasm/admission.js"),
    wasmPath = join(output, "wasm/admission_bg.wasm");
  const api = await import(pathToFileURL(modulePath).href);
  api.initSync({ module: await readFile(wasmPath) });
  assert.equal(api.providerKeyId(fixture.cases[0].token), "test-key");
  const keys = JSON.parse(
    api.providerKeySet(new TextEncoder().encode(JSON.stringify({ keys: [fixture.public_key] }))),
  );
  assert.equal(keys.length, 1);
  assert.equal(keys[0].kid, "test-key");
  assert.throws(
    () =>
      api.providerKeySet(
        new TextEncoder().encode(
          JSON.stringify({ keys: [fixture.public_key, fixture.public_key] }),
        ),
      ),
    (error) => String(error) === "invalid provider assertion",
  );
  const check = (result, f) =>
    assert.deepEqual(
      result,
      { verified: f.expected, error: f.expected === null ? "invalid provider assertion" : null },
      f.name,
    );
  const nodeResults = [];
  for (const f of fixture.cases) {
    let verified = null,
      error = null;
    try {
      verified = JSON.parse(
        api.verifyProviderAssertion(
          f.token,
          JSON.stringify(f.key),
          JSON.stringify(f.context),
          f.now_ms,
        ),
      );
    } catch (failure) {
      error = String(failure);
    }
    const result = { verified, error };
    check(result, f);
    nodeResults.push({ name: f.name, ...result });
  }
  await save("node-result.json", {
    passed: true,
    cases: nodeResults.length,
    results: nodeResults,
    scope,
  });
  const require = createRequire(join(root, "infra/cloudflare/package.json"));
  const { Miniflare, convertV4MiniflareOptions } = require("miniflare");
  const entry = join(output, "wasm/provider-worker.mjs");
  await writeFile(entry, entrySource);
  const runtime = new Miniflare(
    convertV4MiniflareOptions({
      modules: [
        { type: "ESModule", path: entry },
        { type: "ESModule", path: modulePath },
        { type: "CompiledWasm", path: wasmPath },
      ],
      modulesRoot: join(output, "wasm"),
      compatibilityDate: "2026-10-18",
    }),
  );
  const workerdResults = [];
  try {
    for (const f of fixture.cases) {
      const response = await runtime.dispatchFetch("http://provider-assertion.test", {
        method: "POST",
        body: JSON.stringify(f),
      });
      assert.equal(response.status, 200);
      const result = await response.json();
      check(result, f);
      workerdResults.push({ name: f.name, ...result });
    }
  } finally {
    await runtime.dispose();
  }
  await save("workerd-result.json", {
    passed: true,
    cases: workerdResults.length,
    results: workerdResults,
    scope,
  });
  let hostResult;
  if (host) {
    const database = process.env.ZOEN_TEST_PG ?? process.env.DATABASE_URL;
    assert.ok(database, "Set ZOEN_TEST_PG or DATABASE_URL for the isolated native provider oracle");
    const fixturePath = join(output, "native-provider-enrollment.json");
    await run(
      "cargo",
      [
        "test",
        "--quiet",
        "--locked",
        "-p",
        "zoen-relay",
        "--lib",
        "auth::storage::provider_fixture::write_native_provider_enrollment_fixture",
        "--",
        "--ignored",
        "--exact",
      ],
      true,
      { ...process.env, ZOEN_TEST_PG: database, ZOEN_PROVIDER_FIXTURE: fixturePath },
    ).then((text) => writeFile(join(output, "native-provider-oracle.log"), text));
    const nativeHost = JSON.parse(await readFile(fixturePath, "utf8"));
    assert.equal(nativeHost.schema, 1);
    assert.equal(nativeHost.scenarios.length, 20);
    assert.equal(nativeHost.android.scenarios.length, 3);
    const tapPath = join(output, "provider-host-workerd.tap");
    const tap = await run(
      process.execPath,
      [
        "--test",
        "--test-reporter=tap",
        "--test-concurrency=1",
        "infra/cloudflare/relay-state/tests/provider-state.test.mjs",
      ],
      true,
      { ...process.env, ZOEN_PROVIDER_FIXTURE: fixturePath },
    ).catch(async (error) => {
      if (typeof error.stdout === "string") await writeFile(tapPath, error.stdout);
      throw error;
    });
    await writeFile(tapPath, tap);
    process.stdout.write(tap);
    const totals = {};
    for (const name of ["tests", "pass", "fail", "cancelled", "skipped", "todo"]) {
      const matches = [...tap.matchAll(new RegExp(`^# ${name} (\\d+)\\r?$`, "gm"))];
      assert.ok(matches.length, `Missing provider TAP ${name} total`);
      totals[name] = Number(matches.at(-1)[1]);
    }
    assert.ok(totals.tests >= 20, "All twenty native scenarios and host regressions must run");
    assert.equal(totals.pass, totals.tests);
    for (const name of ["fail", "cancelled", "skipped", "todo"]) assert.equal(totals[name], 0);
    hostResult = {
      nativeSqlScenarios: nativeHost.scenarios.length,
      androidNativeSqlScenarios: nativeHost.android.scenarios.length,
      workerd: totals,
      artifacts: await Promise.all(
        [fixturePath, tapPath, join(output, "native-provider-oracle.log")].map(artifact),
      ),
    };
  }
  for (const source of sources)
    assert.deepEqual(
      await artifact(source.path),
      source,
      `Source changed during verification: ${source.path}`,
    );
  await save("summary.json", {
    passed: true,
    scope,
    source: {
      commit: process.env.GITHUB_SHA ?? git("rev-parse", "HEAD"),
      checkoutHead: git("rev-parse", "HEAD"),
      dirty: Boolean(git("status", "--porcelain", "--", ...sourceScopes)),
    },
    sources,
    nativeCases: 18,
    nodeCases: 18,
    workerdCases: 18,
    skipBuild,
    ...(hostResult ? { host: hostResult } : {}),
    artifacts: await Promise.all(
      [
        "native-fixtures.json",
        "node-result.json",
        "workerd-result.json",
        "wasm/admission.js",
        "wasm/admission_bg.wasm",
        "wasm/provider-worker.mjs",
      ].map((name) => artifact(join(output, name))),
    ),
  });
  console.log(
    `PASS provider assertion policy:18native/18Node/18workerd; ${join(output, "summary.json")}`,
  );
} catch (error) {
  await save("summary.json", { passed: false, scope, error: error.message, sources });
  throw error;
}
