#!/usr/bin/env node
// Isolated native PostgreSQL oracle versus a private Rust/WASM Directory DO.
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { createHash } from "node:crypto";
import { copyFile, mkdir, mkdtemp, readFile, readdir, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
let output;
let skipBuild = false;
let media = false;
let transfer = false;
let reads = false;
let profiles = false;
const args = process.argv.slice(2);
for (let index = 0; index < args.length; index++) {
  if (args[index] === "--skip-build") skipBuild = true;
  else if (args[index] === "--media") media = true;
  else if (args[index] === "--transfer") transfer = true;
  else if (args[index] === "--reads") reads = true;
  else if (args[index] === "--profiles") profiles = true;
  else if (args[index] === "--out-dir" && args[index + 1] && !args[index + 1].startsWith("--"))
    output = resolve(args[++index]);
  else
    throw new Error(
      "Usage: verify-cloudflare-directory.mjs [--out-dir DIR] [--skip-build] [--media | --transfer | --reads | --profiles]",
    );
}
assert.ok(
  Number(media) + Number(transfer) + Number(reads) + Number(profiles) <= 1,
  "Choose a single adapter scope",
);
const http = media || transfer;
const label = transfer ? "transfer" : "media";
output ??= await mkdtemp(join(tmpdir(), "zoen-cloudflare-directory-"));
await mkdir(output, { recursive: true });
assert.deepEqual(await readdir(output), [], "Use a fresh empty evidence directory");
const environment = { ...process.env, CARGO_BUILD_JOBS: process.env.CARGO_BUILD_JOBS ?? "2" };
const scope = profiles
  ? "Private signed encrypted profiles and agreement keys on the fixed authoritative Directory: actual native PostgreSQL versus shared Rust/WASM and SQLite, certified active devices, first500 IDs, exact BIGINT versions and monotonic ciphertext updates. No public endpoint, profile-key distribution, ProfileChanged fanout, account recovery or full client acceptance."
  : reads
    ? "Private Person-directory read adapter: actual native PostgreSQL lookup/profile/device results versus shared Rust/WASM and SQLite; default account Lookup/DeliverLink quota, own-device revoked-inclusive enrollment order, first500 IDs and literal prefix LIMIT20. No signed GetProfiles, agent enrollment, public routing, MLS Space sync or socket close acceptance."
    : transfer
      ? "Private encrypted history-transfer HTTP/R2 adapter on the fixed Directory: actual native HTTP responses versus shared Rust/WASM, active devices, shared media quota, atomic create-only chunks, exact-prefix deletion and capability reads. No public endpoint, remote R2, recovery enrollment or full history UI acceptance."
      : media
        ? "Private media HTTP/R2 adapter on the same fixed Directory. Native actual HTTP oracle versus shared Rust/WASM, current device rows, weighted quota and immutable encrypted objects. No public HTTP, real R2 write or deployment. Private one-body/8-waiter/10s-idle/default-quota/25MiB-read bounds; no transfer or backup."
        : "Private single-cell Person/device directory, Directory-owned native binary sockets and MLS key-package publication/claims (128 records/768 KiB response bound). Native PostgreSQL policy versus shared Rust/WASM and SQLite transactions; no public endpoint, Space audience authorization, provider auth, recovery or agent registration.";
const sourcePaths = [
  "Cargo.toml",
  "Cargo.lock",
  "crates/relay-admission/Cargo.toml",
  "crates/relay-admission/src/auth.rs",
  "crates/relay-admission/src/auth/fixtures/rsa-test-only.jwk.json",
  "crates/relay-admission/src/auth/fixtures/rsa-test-only.pk8",
  "crates/relay-admission/src/directory.rs",
  "crates/relay-admission/src/directory/key_packages.rs",
  "crates/relay-admission/src/directory/socket.rs",
  "crates/relay-admission/src/directory/reads.rs",
  "crates/relay-admission/src/directory/sealed_profiles.rs",
  "crates/relay-admission/tests/directory_sealed_profiles.rs",
  "crates/relay-admission/tests/directory_reads.rs",
  "crates/relay-admission/tests/directory_socket.rs",
  "crates/relay-admission/tests/directory_key_packages.rs",
  "crates/relay-admission/tests/directory.rs",
  "crates/relay-admission/src/lib.rs",
  "crates/relay-admission/src/relay_socket.rs",
  "crates/relay-admission/tests/relay_socket.rs",
  "crates/zoen-relay/src/db.rs",
  "crates/zoen-relay/src/session.rs",
  "crates/zoen-relay/src/key_package_claims.rs",
  "crates/zoen-relay/examples/cloudflare-directory-fixture.rs",
  "crates/zoen-relay/examples/cloudflare-directory-fixture/reads.rs",
  "crates/zoen-relay/examples/cloudflare-directory-fixture/sealed_profiles.rs",
  "infra/cloudflare/relay-state/src/directory-state.mjs",
  "infra/cloudflare/relay-state/src/directory-index.mjs",
  "infra/cloudflare/relay-state/src/relay-entrypoint.mjs",
  "infra/cloudflare/relay-state/src/relay-route.mjs",
  "infra/cloudflare/relay-state/src/relay-composition.mjs",
  "infra/cloudflare/relay-state/src/relay-index.mjs",
  "infra/cloudflare/relay-state/tests/relay-ingress-fixture.mjs",
  "infra/cloudflare/relay-state/tests/relay-ingress.test.mjs",
  "infra/cloudflare/relay-state/tests/relay-profile-notices.test.mjs",
  "infra/cloudflare/relay-state/wrangler.relay.jsonc",
  "infra/cloudflare/relay-state/wrangler.closed.jsonc",
  "infra/cloudflare/relay-state/tests/directory-fixture.mjs",
  "infra/cloudflare/relay-state/tests/directory-state.test.mjs",
  "infra/cloudflare/relay-state/tests/directory-socket.test.mjs",
  "infra/cloudflare/relay-state/tests/directory-read.test.mjs",
  "infra/cloudflare/relay-state/tests/directory-sealed-profiles.test.mjs",
  "infra/cloudflare/relay-state/wrangler.directory.jsonc",
  "infra/cloudflare/relay-state/package.json",
  "infra/cloudflare/relay-state/DIRECTORY.md",
  "scripts/verify-cloudflare-directory.mjs",
  "scripts/verify-cloudflare-admission.mjs",
  "scripts/build-cloudflare-closed.mjs",
  "infra/cloudflare/relay-state/src/provider-auth.mjs",
  "infra/cloudflare/relay-state/src/media-blobs.mjs",
  "infra/cloudflare/relay-state/src/blob-io.mjs",
  "infra/cloudflare/relay-state/src/blob-quota.mjs",
  "infra/cloudflare/relay-state/src/transfer-blobs.mjs",
  "crates/relay-admission/src/transfer.rs",
  "crates/relay-admission/src/auth/enrollment.rs",
  "crates/relay-admission/src/blobs.rs",
  "crates/relay-admission/src/gcra.rs",
  "infra/cloudflare/package.json",
  "infra/cloudflare/package-lock.json",
];
// Media evidence binds runtime imports and tests, not independently edited docs.
if (http) {
  sourcePaths.splice(sourcePaths.indexOf("infra/cloudflare/relay-state/DIRECTORY.md"), 1);
  sourcePaths.push(
    "crates/relay-admission/tests/blobs.rs",
    "crates/zoen-relay/src/blobs.rs",
    "crates/zoen-relay/src/limits.rs",
    "crates/zoen-relay/tests/http_device_revocation.rs",
    "crates/zoen-relay/tests/http_device_revocation/media_fixture.rs",
    "crates/zoen-relay/tests/http_device_revocation/transfer_fixture.rs",
    "infra/cloudflare/relay-state/tests/media-fixture.mjs",
    "infra/cloudflare/relay-state/tests/blob-io.test.mjs",
    "infra/cloudflare/relay-state/tests/media-blobs.test.mjs",
    "crates/relay-admission/tests/transfer.rs",
    "infra/cloudflare/relay-state/tests/transfer-fixture.mjs",
    "infra/cloudflare/relay-state/tests/transfer-blobs.test.mjs",
  );
}
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
    child.once("close", (code, signal) => {
      const stdout = capture ? Buffer.concat(chunks).toString("utf8") : undefined;
      if (code === 0) accept(stdout);
      else reject(Object.assign(new Error(`${command} failed (${signal ?? code})`), { stdout }));
    });
  });
}
function testTotals(tap) {
  const totals = {};
  for (const name of ["tests", "pass", "fail", "cancelled", "skipped", "todo"]) {
    const matches = [...tap.matchAll(new RegExp(`^# ${name} (\\d+)\\r?$`, "gm"))];
    assert.ok(matches.length, `Missing Node TAP ${name} total`);
    totals[name] = Number(matches.at(-1)[1]);
  }
  assert.ok(totals.tests > 0, "The workerd suite must run actual cases");
  assert.equal(totals.pass, totals.tests, "Every workerd case must pass");
  for (const name of ["fail", "cancelled", "skipped", "todo"])
    assert.equal(totals[name], 0, `Unexpected workerd ${name}`);
  return totals;
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
  const database = environment.ZOEN_TEST_PG ?? environment.DATABASE_URL;
  if (!database)
    throw new Error("Set ZOEN_TEST_PG or DATABASE_URL to an existing PostgreSQL test database");
  const transitive = (
    await run(
      "git",
      [
        "ls-files",
        "--cached",
        "--others",
        "--exclude-standard",
        "--",
        "crates/relay-admission/src",
        "crates/event-log/src",
        "crates/protocol/src",
        "crates/mls/src",
        "crates/types/src",
        ...(http ? ["crates/zoen-relay/src", "crates/zoen-relay/migrations"] : []),
      ],
      { capture: true },
    )
  )
    .trim()
    .split("\n")
    .filter(Boolean);
  sourcePaths.push(
    ...transitive,
    ...["event-log", "protocol", "mls", "types"].map(
      (name) => `crates/${name}/Cargo.toml`,
    ),
  );
  sourcePaths.splice(0, sourcePaths.length, ...new Set(sourcePaths));
  const beforeSources = await Promise.all(sourcePaths.map((path) => artifact(join(root, path))));
  const runtimePaths = ["admission.js", "admission_bg.wasm"].map((name) =>
    join(root, "infra/cloudflare/relay-state/generated/wasm", name),
  );
  const beforeRuntime = await Promise.all(runtimePaths.map(artifact));
  let fixture;
  let nativeHttp;
  const fixturePath = join(
    output,
    http ? `${label}-native-fixtures.json` : "directory-native-fixtures.json",
  );
  if (http) {
    const nativeLog = await run(
      "cargo",
      [
        "test",
        "--quiet",
        "--locked",
        "-p",
        "zoen-relay",
        "--test",
        "http_device_revocation",
        "--",
        ...(transfer ? ["cloudflare_transfer_native_fixture"] : ["--include-ignored"]),
        "--test-threads=1",
      ],
      {
        capture: true,
        env: {
          ...environment,
          ZOEN_TEST_PG: database,
          [transfer ? "ZOEN_TRANSFER_FIXTURE_OUTPUT" : "ZOEN_MEDIA_FIXTURE_OUTPUT"]: fixturePath,
        },
      },
    ).catch(async (failure) => {
      if (typeof failure.stdout === "string")
        await writeFile(join(output, `${label}-native-http.log`), failure.stdout);
      throw failure;
    });
    await writeFile(join(output, `${label}-native-http.log`), nativeLog);
    const totals = /^test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;/m.exec(nativeLog);
    assert.ok(totals, "Native HTTP runner must report actual passing tests");
    nativeHttp = { pass: Number(totals[1]), fail: Number(totals[2]), ignored: Number(totals[3]) };
    assert.ok(nativeHttp.pass >= (transfer ? 1 : 6));
    assert.equal(nativeHttp.fail, 0);
    assert.equal(nativeHttp.ignored, 0);
    fixture = JSON.parse(await readFile(fixturePath, "utf8"));
    assert.equal(fixture.scope, `native-${label}-http`);
    assert.ok(
      fixture.steps.length >= (transfer ? 30 : 28),
      "Every actual native HTTP boundary must run",
    );
    for (const label of transfer
      ? [
          "max",
          "max-plus-one-before-signature",
          "extractor-limit",
          "invalid-utf8",
          "delete",
          "revoked-fresh",
          "revoked-delete",
          "immutable-original",
        ]
      : [
          "max",
          "max-plus-one",
          "extractor-limit",
          "extractor-limit-auth-first",
          "percent-encoded-hash",
          "malformed-percent",
          "invalid-utf8",
          "encoded-slash",
          "long-bad-hash",
          "long-bad-device",
          "long-bad-signature",
          "long-leading-zero-timestamp",
        ])
      assert.ok(
        fixture.steps.some((step) => step.label === label),
        `Missing native media case: ${label}`,
      );
    if (media)
      assert.equal(
        fixture.storage_failures.length,
        3,
        "Native put/get/read storage failures required",
      );
  } else {
    fixture = JSON.parse(
      await run(
        "cargo",
        [
          "run",
          "--quiet",
          "--locked",
          "-p",
          "zoen-relay",
          "--example",
          "cloudflare-directory-fixture",
        ],
        {
          capture: true,
          env: { ...environment, ZOEN_TEST_PG: database },
        },
      ),
    );
    assert.equal(
      fixture.steps.length,
      71,
      "Every native directory and key-package scenario must run",
    );
    await writeFile(fixturePath, `${JSON.stringify(fixture, null, 2)}\n`);
  }
  assert.equal(fixture.schema, 1);
  const workerdTapPath = join(output, http ? `${label}-workerd.tap` : "directory-workerd.tap");
  const testFiles = profiles
    ? ["infra/cloudflare/relay-state/tests/directory-sealed-profiles.test.mjs"]
    : reads
      ? ["infra/cloudflare/relay-state/tests/directory-read.test.mjs"]
      : transfer
        ? ["infra/cloudflare/relay-state/tests/transfer-blobs.test.mjs"]
        : media
          ? [
              "infra/cloudflare/relay-state/tests/blob-io.test.mjs",
              "infra/cloudflare/relay-state/tests/media-blobs.test.mjs",
            ]
          : [
              "infra/cloudflare/relay-state/tests/directory-state.test.mjs",
              "infra/cloudflare/relay-state/tests/directory-read.test.mjs",
              "infra/cloudflare/relay-state/tests/directory-sealed-profiles.test.mjs",
              "infra/cloudflare/relay-state/tests/directory-socket.test.mjs",
              "infra/cloudflare/relay-state/tests/relay-ingress.test.mjs",
            ];
  const fixtureEnv = transfer
    ? { ZOEN_TRANSFER_FIXTURE: fixturePath }
    : media
      ? { ZOEN_MEDIA_FIXTURE: fixturePath }
      : { ZOEN_DIRECTORY_FIXTURE: fixturePath };
  // These use real SQLite/WASM with scheduling doubles, not workerd sockets.
  let profileNoticeUnits = null;
  if (profiles || (!reads && !http)) {
    const path = join(output, "profile-notice-units.tap");
    const tap = await run(
      process.execPath,
      [
        "--test",
        "--test-reporter=tap",
        "infra/cloudflare/relay-state/tests/relay-profile-notices.test.mjs",
      ],
      { capture: true, env: { ...environment, ...fixtureEnv } },
    ).catch(async (failure) => {
      if (typeof failure.stdout === "string") {
        await writeFile(path, failure.stdout);
        process.stdout.write(failure.stdout);
      }
      throw failure;
    });
    await writeFile(path, tap);
    process.stdout.write(tap);
    profileNoticeUnits = { ...testTotals(tap), artifact: await artifact(path) };
  }
  const workerdTap = await run(
    process.execPath,
    ["--test", "--test-reporter=tap", "--test-concurrency=1", ...testFiles],
    {
      capture: true,
      env: { ...environment, ...fixtureEnv },
    },
  ).catch(async (failure) => {
    if (typeof failure.stdout === "string") {
      await writeFile(workerdTapPath, failure.stdout);
      process.stdout.write(failure.stdout);
    }
    throw failure;
  });
  await writeFile(workerdTapPath, workerdTap);
  process.stdout.write(workerdTap);
  const workerd = testTotals(workerdTap);
  const afterSources = await Promise.all(sourcePaths.map((path) => artifact(join(root, path))));
  assert.deepEqual(
    afterSources,
    beforeSources,
    "Runtime/test sources must remain frozen throughout validation",
  );
  assert.deepEqual(
    await Promise.all(runtimePaths.map(artifact)),
    beforeRuntime,
    "Runtime WASM/glue must remain frozen throughout validation",
  );
  const copied = join(output, "wasm");
  await mkdir(copied, { recursive: true });
  for (const name of ["admission.js", "admission_bg.wasm"])
    await copyFile(
      join(root, "infra/cloudflare/relay-state/generated/wasm", name),
      join(copied, name),
    );
  const checkoutHead = (await run("git", ["rev-parse", "HEAD"], { capture: true })).trim();
  const dirty = Boolean(
    (await run("git", ["status", "--porcelain", "--", ...sourcePaths], { capture: true })).trim(),
  );
  await summary({
    passed: true,
    scope,
    output,
    skipBuild,
    media,
    transfer,
    reads,
    profiles,
    nativeHttp,
    nativeSteps: fixture.steps.length,
    ...(reads ? { nativeReadSteps: fixture.reads.steps.length } : {}),
    ...(profiles
      ? {
          nativeSealedProfileSteps: fixture.sealed_profiles.steps.length,
          nativeCrypto: fixture.sealed_profiles.crypto_proof,
        }
      : {}),
    workerdCases: workerd.tests,
    workerd,
    profileNoticeUnits,
    source: {
      commit: process.env.GITHUB_SHA ?? checkoutHead,
      checkoutHead,
      dirty,
      dirtyScope: sourcePaths,
    },
    sources: afterSources,
    artifacts: await Promise.all(
      [
        fixturePath,
        workerdTapPath,
        ...(http ? [join(output, `${label}-native-http.log`)] : []),
        join(copied, "admission.js"),
        join(copied, "admission_bg.wasm"),
      ].map(artifact),
    ),
  });
  console.log(`Private directory verification passed; receipt ${join(output, "summary.json")}`);
} catch (failure) {
  await summary({ passed: false, scope, output, skipBuild, error: failure.message });
  throw failure;
}
