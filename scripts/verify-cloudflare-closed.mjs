#!/usr/bin/env node
// Private Direct/Group admission and MLS oracle; uses only fresh owned FDB cells.
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { createHash } from "node:crypto";
import { copyFile, mkdir, mkdtemp, readFile, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
let output;
let skipBuild = false;
const args = process.argv.slice(2);
for (let i = 0; i < args.length; i++) {
  if (args[i] === "--skip-build") skipBuild = true;
  else if (args[i] === "--out-dir" && args[i + 1] && !args[i + 1].startsWith("--"))
    output = resolve(args[++i]);
  else throw new Error("Usage: verify-cloudflare-closed.mjs [--out-dir DIR] [--skip-build]");
}
output ??= await mkdtemp(join(tmpdir(), "zoen-cloudflare-closed-"));
await mkdir(output, { recursive: true });
const generated = join(root, "infra/cloudflare/relay-state/generated");
const fixturePath = join(output, "closed-native-fixtures.json");
const scope =
  "Private Direct/Group Closed and EndToEnd projection: actual native FDB and core MLS versus Rust/WASM and SQLite Durable Object transactions, including epochs, own-device Welcomes, holds, checkpoints, bounded ciphertext pruning and authorized byte-bounded read pages. Separate SQLite/WASM control-plane unit cases cover publication preparation and reference-outbox faults with host doubles. No public protocol, directory/revocation authentication, global invite resolver, actual socket fanout, backup recovery or full encrypted sync acceptance.";
const environment = { ...process.env, CARGO_BUILD_JOBS: process.env.CARGO_BUILD_JOBS ?? "2" };

function run(command, commandArgs, { capture = false, env = environment } = {}) {
  console.log(`Running ${command} ${commandArgs.join(" ")}`);
  return new Promise((accept, reject) => {
    const child = spawn(command, commandArgs, {
      cwd: root,
      env,
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
function testTotals(tap) {
  const totals = {};
  for (const name of ["tests", "pass", "fail", "cancelled", "skipped", "todo"]) {
    const matches = [...tap.matchAll(new RegExp(`^# ${name} (\\d+)\\r?$`, "gm"))];
    assert.ok(matches.length, `Missing Node TAP ${name} total`);
    totals[name] = Number(matches.at(-1)[1]);
  }
  assert.ok(totals.tests > 0, "Actual test cases must run");
  assert.equal(totals.pass, totals.tests, "Every case must pass");
  for (const name of ["fail", "cancelled", "skipped", "todo"])
    assert.equal(totals[name], 0, `Unexpected ${name}`);
  return totals;
}
async function testSuite(label, files, env = environment) {
  const path = join(output, `${label}.tap`);
  const tap = await run(
    process.execPath,
    ["--test", "--test-reporter=tap", "--test-concurrency=1", ...files],
    { capture: true, env },
  ).catch(async (failure) => {
    if (typeof failure.stdout === "string") {
      await writeFile(path, failure.stdout);
      process.stdout.write(failure.stdout);
    }
    throw failure;
  });
  await writeFile(path, tap);
  process.stdout.write(tap);
  return { ...testTotals(tap), artifact: await artifact(path) };
}
async function artifact(path) {
  const bytes = await readFile(path);
  return {
    path,
    bytes: bytes.byteLength,
    sha256: createHash("sha256").update(bytes).digest("hex"),
  };
}
async function summary(value) {
  await writeFile(join(output, "summary.json"), `${JSON.stringify(value, null, 2)}\n`);
}

try {
  if (!skipBuild) await run(process.execPath, [join(root, "scripts/build-cloudflare-closed.mjs")]);
  const native = await run(
    "cargo",
    ["run", "--quiet", "--locked", "-p", "zoen-relay", "--example", "cloudflare-closed-fixture"],
    { capture: true },
  );
  const fixture = JSON.parse(native);
  assert.equal(typeof fixture.space_id, "string");
  assert.equal(fixture.steps.length, 17, "All native differential steps must run");
  assert.equal(fixture.unsupported.length, 4, "All unsupported/signature no-write cases must run");
  assert.equal(fixture.encrypted.length, 2, "Actual core MLS Direct and Group cases must run");
  const encryptedSteps = fixture.encrypted.reduce((sum, item) => sum + item.steps.length, 0);
  const actualPeerOpens = fixture.encrypted.reduce((sum, item) => sum + item.actual_peer_opens, 0);
  assert.equal(
    encryptedSteps,
    302,
    "All native encrypted transitions and bounded pruning steps must run",
  );
  assert.equal(actualPeerOpens, 275, "Every actual encrypted application must open on a peer");
  assert.ok(
    fixture.encrypted.some((item) => item.cold_native_epoch_checked),
    "Native durable epoch reload must run",
  );
  assert.equal(fixture.authorized_reads.length, 1, "Actual native authorized-read case must run");
  assert.equal(fixture.authorized_reads[0].actual_peer_opens, 65);
  assert.ok(fixture.authorized_reads[0].byte_limited_pages_before_removal > 1);
  const authorizedReadVectors = fixture.authorized_reads[0].steps.filter(
    (step) => step.op === "read_for",
  ).length;
  await writeFile(fixturePath, `${JSON.stringify(fixture, null, 2)}\n`);
  const storageTests = await testSuite("storage-contract", [
    "infra/cloudflare/relay-state/test/state.test.mjs",
  ]);
  const fixtureEnv = { ...environment, ZOEN_CLOSED_FIXTURE: fixturePath };
  const workerd = await testSuite(
    "closed-workerd",
    ["infra/cloudflare/relay-state/tests/closed-state.test.mjs"],
    fixtureEnv,
  );
  const reads = await testSuite(
    "authorized-reads-workerd",
    ["infra/cloudflare/relay-state/tests/closed-reads.test.mjs"],
    fixtureEnv,
  );
  const publicationUnits = await testSuite(
    "publication-control-units",
    ["infra/cloudflare/relay-state/tests/relay-publications.test.mjs"],
    fixtureEnv,
  );
  const copied = join(output, "wasm");
  await mkdir(copied, { recursive: true });
  for (const name of ["admission.js", "admission_bg.wasm"])
    await copyFile(join(generated, "wasm", name), join(copied, name));
  const sourcePaths = [
    "Cargo.toml",
    "Cargo.lock",
    "crates/relay-admission",
    "crates/event-log",
    "crates/protocol",
    "crates/mls",
    "crates/types",
    "crates/zoen-relay",
    "infra/cloudflare/relay-state",
    "scripts/verify-cloudflare-admission.mjs",
    "scripts/build-cloudflare-closed.mjs",
    "scripts/verify-cloudflare-closed.mjs",
  ];
  const checkoutHead = (await run("git", ["rev-parse", "HEAD"], { capture: true })).trim();
  const dirty = Boolean(
    (await run("git", ["status", "--porcelain", "--", ...sourcePaths], { capture: true })).trim(),
  );
  const result = {
    passed: true,
    output,
    skipBuild,
    scope,
    nativeSteps: fixture.steps.length,
    negativeCases: fixture.unsupported.length,
    encryptedCases: fixture.encrypted.length,
    encryptedSteps,
    actualPeerOpens,
    workerdCases: workerd.tests,
    workerd,
    authorizedReadCases: fixture.authorized_reads.length,
    authorizedReadVectors,
    authorizedReadActualPeerOpens: fixture.authorized_reads[0].actual_peer_opens,
    authorizedReadWorkerdCases: reads.tests,
    reads,
    sharedStorageContractCases: storageTests.tests,
    storageTests,
    publicationControlUnitCases: publicationUnits.tests,
    publicationUnits,
    source: {
      commit: process.env.GITHUB_SHA ?? checkoutHead,
      checkoutHead,
      dirty,
      dirtyScope: sourcePaths,
    },
    artifacts: {
      nativeFixtures: await artifact(fixturePath),
      javascript: await artifact(join(copied, "admission.js")),
      wasm: await artifact(join(copied, "admission_bg.wasm")),
      runner: await artifact(fileURLToPath(import.meta.url)),
      nativeOracle: await artifact(
        join(root, "crates/zoen-relay/examples/cloudflare-closed-fixture.rs"),
      ),
      nativeMlsOracle: await artifact(
        join(root, "crates/zoen-relay/examples/cloudflare-closed-fixture/mls.rs"),
      ),
      sharedMlsProjection: await artifact(
        join(root, "crates/relay-admission/src/mls_projection.rs"),
      ),
      sharedClosedProjection: await artifact(
        join(root, "crates/relay-admission/src/closed.rs"),
      ),
      sharedReadPolicy: await artifact(
        join(root, "crates/relay-admission/src/closed/reads.rs"),
      ),
      sharedPublicationPolicy: await artifact(
        join(root, "crates/relay-admission/src/closed/relay.rs"),
      ),
      publicationControl: await artifact(
        join(root, "infra/cloudflare/relay-state/src/relay-publications.mjs"),
      ),
      publicationControlUnitTests: await artifact(
        join(root, "infra/cloudflare/relay-state/tests/relay-publications.test.mjs"),
      ),
      nativeReadOracle: await artifact(
        join(root, "crates/zoen-relay/examples/cloudflare-closed-fixture/reads.rs"),
      ),
      readWorkerdTests: await artifact(
        join(root, "infra/cloudflare/relay-state/tests/closed-reads.test.mjs"),
      ),
      nativeLog: await artifact(join(root, "crates/zoen-relay/src/log/fdb.rs")),
      sqlAdapter: await artifact(join(root, "infra/cloudflare/relay-state/src/closed-space.mjs")),
      storage: await artifact(join(root, "infra/cloudflare/relay-state/src/space-storage.mjs")),
      fixtureBridge: await artifact(
        join(root, "infra/cloudflare/relay-state/tests/closed-fixture.mjs"),
      ),
      workerdTests: await artifact(
        join(root, "infra/cloudflare/relay-state/tests/closed-state.test.mjs"),
      ),
    },
  };
  await summary(result);
  console.log(JSON.stringify(result));
} catch (error) {
  await summary({
    passed: false,
    output,
    skipBuild,
    scope,
    error: error instanceof Error ? error.message : String(error),
  });
  throw error;
}
