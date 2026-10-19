#!/usr/bin/env node
// Preparation is read-only with respect to Cloudflare. Execution is deliberately
// opt-in and owns ONE fresh private Worker/SQLite namespace, deleted in finally.
// Usage: node scripts/verify-cloudflare-directory-remote.mjs --wasm-dir DIR
//   [--out-dir NEW_DIR] [--execute --client-bin BIN --client-sha256 SHA256]
// Media preparation: --media [--media-source FULL_SHA --media-version UUID].
// Media execution additionally needs those exact approved source/version flags
// and ZOEN_CF_REMOTE_MEDIA_QA=1; it NEVER builds/deploys/provisions or deletes R2.
// Execution additionally requires ZOEN_CF_REMOTE_DIRECTORY_QA=1. Build the native
// example separately under the shared compute lease; this runner never invokes
// Cargo, reads a user vault, modifies existing Workers or enables public routes.
import assert from "node:assert/strict";
import { execFileSync, spawn } from "node:child_process";
import { createHash, randomBytes } from "node:crypto";
import {
  copyFile,
  mkdir,
  mkdtemp,
  readFile,
  readdir,
  rm,
  statfs,
  writeFile,
} from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join, posix, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const candidate = "8d64d762780a97b7d50c4c2bf23d0172061ebdf3";
const account = "86e8a5b2686413989e8bef1f75c29079";
const expectedWasm = {
  "admission.js": {
    bytes: 25_495,
    sha256: "454fff7f46ad128ae6cf47d102eb8e4f21d05e8eff0f14ca9eabd6f10d0e024d",
  },
  "admission_bg.wasm": {
    bytes: 1_787_533,
    sha256: "5f9011928d6b56b92039c093debd9934a8d57978c939d21ea146146cd90bd9cd",
  },
};
const args = process.argv.slice(2);
let output;
let wasmDir;
let clientBin;
let clientSha;
let execute = false;
let mediaMode = false;
let mediaSource;
let mediaVersion;
for (let index = 0; index < args.length; index++) {
  const value = args[index];
  if (value === "--execute") execute = true;
  else if (value === "--media") mediaMode = true;
  else if (
    [
      "--out-dir",
      "--wasm-dir",
      "--client-bin",
      "--client-sha256",
      "--media-source",
      "--media-version",
    ].includes(value) &&
    args[index + 1] &&
    !args[index + 1].startsWith("--")
  ) {
    const next = args[++index];
    if (value === "--out-dir") output = resolve(next);
    if (value === "--wasm-dir") wasmDir = resolve(next);
    if (value === "--client-bin") clientBin = resolve(next);
    if (value === "--client-sha256") clientSha = next;
    if (value === "--media-source") mediaSource = next;
    if (value === "--media-version") mediaVersion = next;
  } else
    throw new Error(
      "Usage: verify-cloudflare-directory-remote.mjs --wasm-dir DIR [--out-dir NEW_DIR] [--execute --client-bin BIN --client-sha256 SHA256]",
    );
}
if (!mediaMode) assert.ok(wasmDir, "Supply the sealed reviewed WASM directory");
if (!mediaMode)
  assert.ok(!mediaSource && !mediaVersion, "Media flags require explicit --media mode");
if (mediaSource)
  assert.match(mediaSource, /^[a-f0-9]{40}$/, "Exact reviewed media commit required");
if (mediaVersion)
  assert.match(
    mediaVersion,
    /^[a-f0-9]{8}(?:-[a-f0-9]{4}){3}-[a-f0-9]{12}$/,
    "Exact approved private media version required",
  );
if (execute) {
  assert.equal(
    process.env[mediaMode ? "ZOEN_CF_REMOTE_MEDIA_QA" : "ZOEN_CF_REMOTE_DIRECTORY_QA"],
    "1",
    "Explicit private remote QA gate required",
  );
  if (mediaMode)
    assert.ok(
      mediaSource && mediaVersion,
      "Media execution requires reviewed source and deployed version",
    );
  assert.ok(
    clientBin && /^[a-f0-9]{64}$/.test(clientSha ?? ""),
    "Supply the reviewed native client binary and SHA256",
  );
}
if (output) {
  await mkdir(output, { recursive: true });
  assert.deepEqual(await readdir(output), [], "Output must be a NEW empty evidence directory");
} else output = await mkdtemp(join(tmpdir(), "zoen-directory-remote-"));
const worker = mediaMode
  ? "zoen-relay-directory"
  : `zoen-directory-socket-qa-v99-${randomBytes(5).toString("hex")}`;
const cell = mediaMode
  ? `qa-media-v99-${randomBytes(8).toString("hex")}`
  : "one-owned-synthetic-directory";
const cli = join(root, "infra/cloudflare/node_modules/wrangler/bin/wrangler.js");
const environment = {
  ...process.env,
  WRANGLER_WRITE_LOGS: "false",
  WRANGLER_LOG: "none",
  WRANGLER_SEND_METRICS: "false",
  WRANGLER_LOG_SANITIZE: "true",
  NO_COLOR: "1",
};
const digest = (bytes) => createHash("sha256").update(bytes).digest("hex");
const save = (name, value) =>
  writeFile(join(output, `${name}.json`), `${JSON.stringify(value, null, 2)}\n`);
async function artifact(path) {
  const bytes = await readFile(path);
  return { path, bytes: bytes.length, sha256: digest(bytes) };
}
async function diskGuard() {
  const disk = await statfs(output, { bigint: true });
  assert.ok(disk.bavail * disk.bsize >= 5n * 1024n ** 3n, "Keep at least 5 GiB free");
}
async function runOwned(
  name,
  command,
  commandArgs,
  input,
  {
    timeoutMs = name === "native-init" ? 5000 : 180_000,
    capture = name === "native-init",
    env = environment,
  } = {},
) {
  const result = await new Promise((accept, reject) => {
    // Each subprocess has an OWN process group. On timeout, terminate only
    // that group, including Wrangler's owned local preview/runtime children.
    const child = spawn(command, commandArgs, {
      cwd: root,
      env,
      detached: true,
      stdio: ["pipe", "pipe", "pipe"],
    });
    let foreignOutputBytes = 0;
    let timedOut = false;
    let killTimer;
    const terminate = (signal) => {
      try {
        process.kill(-child.pid, signal);
      } catch (error) {
        if (error.code !== "ESRCH") throw error;
      }
    };
    const timer = setTimeout(() => {
      timedOut = true;
      terminate("SIGTERM");
      killTimer = setTimeout(() => terminate("SIGKILL"), 5000);
    }, timeoutMs);
    const chunks = [];
    child.stdout.on("data", (chunk) => {
      foreignOutputBytes += chunk.length;
      if (capture) chunks.push(chunk);
    });
    child.stderr.on("data", (chunk) => {
      foreignOutputBytes += chunk.length;
    });
    child.once("error", () => {
      clearTimeout(timer);
      clearTimeout(killTimer);
      reject(new Error("owned subprocess startup failed"));
    });
    child.once("close", (code, signal) => {
      clearTimeout(timer);
      clearTimeout(killTimer);
      accept({
        code,
        signal,
        timedOut,
        foreignOutputBytes,
        stdout: Buffer.concat(chunks).toString(),
      });
    });
    child.stdin.on("error", () => {});
    child.stdin.end(input ?? "");
  });
  // CLI or preview output can contain bearer URLs/tokens. Never log it, even
  // on failure. Auth JSON stays in memory; only the native-init public fixture
  // is explicitly parsed and saved below.
  await writeFile(
    join(output, `${name}.log`),
    `${JSON.stringify({
      exit: result.code,
      signal: result.signal,
      timedOut: result.timedOut,
      timeoutMs,
      foreignOutputBytes: result.foreignOutputBytes,
      foreignOutputWithheld: true,
    })}\n`,
  );
  assert.equal(result.timedOut, false, "Owned subprocess exceeded its bound");
  assert.equal(result.code, 0, "Owned subprocess failed");
  return result.stdout;
}

async function runMediaMode() {
  const namespace = "66f8e984ecda4663944a760b49d31254";
  const runtimePaths = [
    "infra/cloudflare/relay-state/wrangler.directory.jsonc",
    "infra/cloudflare/relay-state/src/directory-index.mjs",
    "infra/cloudflare/relay-state/src/directory-state.mjs",
    "infra/cloudflare/relay-state/src/media-blobs.mjs",
    "infra/cloudflare/relay-state/src/provider-auth.mjs",
    "crates/roda-relay-admission/src/blobs.rs",
    "crates/roda-log/src/media.rs",
    "crates/roda-proto/src/lib.rs",
  ];
  const sources = [];
  let config;
  // Follow the selected commit's module imports rather than adding today's
  // files to a historical proof whose source tree never contained them.
  const include = (relative) => {
    if (!runtimePaths.includes(relative)) runtimePaths.push(relative);
  };
  for (const relative of runtimePaths) {
    const bytes = mediaSource
      ? execFileSync("git", ["show", `${mediaSource}:${relative}`], {
          cwd: root,
          stdio: ["ignore", "pipe", "pipe"],
        })
      : await readFile(join(root, relative));
    sources.push({
      path: relative,
      sourceCommit: mediaSource ?? null,
      bytes: bytes.length,
      sha256: digest(bytes),
    });
    if (relative.endsWith("wrangler.directory.jsonc")) config = JSON.parse(bytes.toString());
    if (relative.startsWith("infra/cloudflare/relay-state/src/")) {
      const imports = /(?:\bfrom\s*|\bimport\s*(?:\(\s*)?)["'](\.{1,2}\/[^"']+\.mjs)["']/g;
      for (const match of bytes.toString().matchAll(imports)) {
        const dependency = posix.join(posix.dirname(relative), match[1]);
        assert.match(
          dependency,
          /^infra\/cloudflare\/relay-state\/src\/(?:[\w-]+\/)*[\w-]+\.mjs$/,
          "Private media module dependency must stay in the reviewed source directory",
        );
        include(dependency);
      }
      if (relative.endsWith("/transfer-blobs.mjs"))
        include("crates/roda-relay-admission/src/transfer.rs");
      if (relative.endsWith("/blob-quota.mjs")) include("crates/roda-relay-admission/src/gcra.rs");
    }
  }
  assert.equal(config.name, worker);
  assert.equal(config.account_id, account);
  assert.equal(config.workers_dev, false);
  assert.equal(config.preview_urls, false);
  assert.deepEqual(config.routes, []);
  assert.equal(config.vars.RELAY_NAME, "zoen-cloudflare-private");
  assert.notEqual(cell, config.vars.DIRECTORY_CELL, "Production fixed cell must remain untouched");
  assert.deepEqual(config.r2_buckets, [
    { binding: "MEDIA_BUCKET", bucket_name: "zoen-private-blobs" },
  ]);
  assert.deepEqual(config.durable_objects.bindings, [
    { name: "DIRECTORY", class_name: "DirectoryState" },
  ]);
  for (const relative of [
    "scripts/verify-cloudflare-directory-remote.mjs",
    "scripts/cloudflare-directory-remote-preview.mjs",
    "infra/cloudflare/relay-state/tests/directory-remote-bridge.mjs",
    "crates/roda-relay-admission/examples/directory-socket-client.rs",
    "infra/cloudflare/package.json",
    "infra/cloudflare/package-lock.json",
  ])
    sources.push({ ...(await artifact(join(root, relative))), helper: true });
  await copyFile(
    join(root, "scripts/cloudflare-directory-remote-preview.mjs"),
    join(output, "preview.mjs"),
  );
  await copyFile(
    join(root, "infra/cloudflare/relay-state/tests/directory-remote-bridge.mjs"),
    join(output, "bridge.mjs"),
  );
  const bridgeConfig = join(output, "bridge.jsonc");
  const base = {
    name: `zoen-directory-media-qa-preview-${randomBytes(5).toString("hex")}`,
    account_id: account,
    main: join(output, "bridge.mjs"),
    compatibility_date: "2026-10-18",
    workers_dev: false,
    preview_urls: false,
    routes: [],
    durable_objects: {
      bindings: [{ name: "DIRECTORY", class_name: "DirectoryState", script_name: worker }],
    },
    services: [{ binding: "PRIVATE", service: worker }],
  };
  await writeFile(
    bridgeConfig,
    JSON.stringify({ ...base, vars: { DIRECTORY_CELL: cell, QA_MEDIA: "1" } }, null, 2),
  );
  await save("plan", {
    mode: "media",
    execute,
    cloudRequestsDuringPreparation: 0,
    sourceState: mediaSource
      ? "immutable-reviewed-commit"
      : "working-tree-source-review-only-not-executable",
    expectedSource: mediaSource ?? null,
    expectedDeployedVersion: mediaVersion ?? null,
    worker,
    namespace,
    cell,
    productionCellUntouched: true,
    bucket: "zoen-private-blobs",
    objectCountMaximum: 2,
    totalCiphertextBytesMaximum: 8192,
    sources,
    provisioning:
      "NONE. No Cargo, build, deploy, Worker/namespace creation, migration, account DB, R2 binding in the preview or cloud DELETE command.",
    preview:
      "Existing namespace, one hardcoded fresh OWN QA cell, normal native socket registration, direct DO.media. PRIVATE.fetch is tested404; fixed service-facade forwarding is local-test scope only.",
    signing:
      "Native media::seal/open and blob_put_message with actual SystemTime; encryption keys stay only in owned0700 home/0600 files. Exactly two native-generated ciphertext hashes allowlisted in preview.",
    checkpoints: [
      "private-preview/outer404",
      "missing404/no-public-cache",
      "normal-own-cell-registration",
      "PUT201/retry200",
      "exactGET/nativeopen",
      "Unlink/fresh+duplicatePUT403",
      "GET-capability-after-Unlink",
    ],
    cleanup:
      "Stop preview/remove own keys; owner CLI deletes/verifies only the two exact listed R2 keys. No SQL/clear/delete RPC. Tiny revoked OWN QA-cell metadata may remain; production fixed cell untouched.",
    limits:
      "No public bucket/HTTP/provider acceptance, facade-forwarding claim, large media/races/quota/performance acceptance. Legacy8d socket pins and proofs remain separate.",
  });
  if (!execute) {
    console.log(
      JSON.stringify({
        prepared: true,
        mode: "media",
        cloudRequests: 0,
        plan: join(output, "plan.json"),
      }),
    );
    return;
  }
  let credential;
  let secretParent;
  let before;
  let failure = null;
  let phase = "media-preflight";
  const cleanup = {
    r2OwnerCleanupPending: true,
    ownQaCellMetadataMayRemain: true,
    productionFixedCellUntouched: true,
  };
  async function api(path) {
    const response = await fetch(
      `https://api.cloudflare.com/client/v4/accounts/${account}${path}`,
      {
        headers: { Authorization: `Bearer ${credential}` },
        signal: AbortSignal.timeout(30_000),
      },
    );
    const body = await response.json();
    assert.equal(body.success, true, "Read-only private metadata failed");
    return body.result;
  }
  async function metadata() {
    const scripts = await api("/workers/scripts");
    const objects = [];
    for (let page = 1; ; page++) {
      const batch = await api(`/workers/durable_objects/namespaces?per_page=100&page=${page}`);
      objects.push(...batch);
      if (batch.length < 100) break;
      assert.ok(page < 100, "Bounded full namespace inventory required");
    }
    assert.ok(
      objects.some(
        (value) =>
          value.id === namespace && value.script === worker && value.class === "DirectoryState",
      ),
    );
    const endpoint = `/workers/services/${worker}/environments/production`;
    const bindings = await api(`${endpoint}/bindings`);
    const rows = Array.isArray(bindings) ? bindings : bindings.bindings;
    assert.equal(rows.filter((value) => value.type === "durable_object_namespace").length, 1);
    assert.ok(rows.some((value) => value.name === "DIRECTORY" && value.namespace_id === namespace));
    assert.ok(
      rows.some(
        (value) => value.name === "MEDIA_BUCKET" && value.bucket_name === "zoen-private-blobs",
      ),
    );
    const routes = await api(`${endpoint}/routes?show_zonename=true`);
    assert.deepEqual(routes, []);
    const subdomain = await api(`${endpoint}/subdomain`);
    assert.equal(subdomain.enabled, false);
    assert.equal(subdomain.previews_enabled, false);
    const deployment = await api(`/workers/scripts/${worker}/deployments`);
    const versions = deployment.deployments[0].versions;
    assert.equal(versions.length, 1);
    assert.equal(versions[0].version_id, mediaVersion);
    assert.equal(versions[0].percentage, 100);
    return {
      scripts: scripts
        .map((value) => ({ id: value.id, etag: value.etag }))
        .sort((a, b) => a.id.localeCompare(b.id)),
      namespaces: objects.sort((a, b) => a.id.localeCompare(b.id)),
      bindings,
      routes,
      subdomain,
      activeVersion: mediaVersion,
    };
  }
  async function assertHelpers() {
    for (const source of sources.filter((value) => value.helper))
      assert.equal(
        (await artifact(source.path)).sha256,
        source.sha256,
        "Media helper source drift",
      );
    assert.equal((await artifact(clientBin)).sha256, clientSha, "Native media client binary drift");
  }
  try {
    await diskGuard();
    await assertHelpers();
    const frozenClient = join(output, "directory-socket-client");
    await copyFile(clientBin, frozenClient);
    assert.equal((await artifact(frozenClient)).sha256, clientSha);
    secretParent = await mkdtemp(join(tmpdir(), "zoen-media-owned-client-"));
    const secretHome = join(secretParent, "keys");
    const fixture = JSON.parse(
      await runOwned(
        "native-init",
        frozenClient,
        [],
        JSON.stringify({ op: "init", home: secretHome }),
      ),
    );
    const media = JSON.parse(
      await runOwned(
        "native-media-init",
        frozenClient,
        [],
        JSON.stringify({ op: "media-init", home: secretHome }),
        { capture: true, timeoutMs: 5000 },
      ),
    );
    assert.equal(media.objects.length, 2);
    assert.ok(media.total_ciphertext_bytes <= 8192);
    let ciphertextBytes = 0;
    for (const object of media.objects) assert.match(object.sha, /^[a-f0-9]{64}$/);
    for (const object of media.objects) {
      const length = Buffer.from(object.ciphertext_base64, "base64").length;
      assert.equal(length, object.ciphertext_bytes);
      ciphertextBytes += length;
    }
    assert.equal(ciphertextBytes, media.total_ciphertext_bytes);
    assert.ok(ciphertextBytes <= 8192);
    assert.notEqual(media.objects[0].sha, media.objects[1].sha);
    const fixturePath = join(output, "native-public-fixture.json");
    const mediaPath = join(output, "native-public-media.json");
    await writeFile(fixturePath, JSON.stringify(fixture, null, 2));
    await writeFile(mediaPath, JSON.stringify(media, null, 2));
    // Persist the exact allowlist BEFORE the first possible R2 write, including
    // partial/failing journeys. Owner cleanup never needs an inferred prefix.
    await save("r2-object-cleanup", {
      bucket: "zoen-private-blobs",
      allowedKeys: media.objects.map((object) => object.object_key),
      ownerCleanupPending: true,
      storedKeysUnknownUntilObservation: true,
      cell,
      noDeleteRpc: true,
    });
    await writeFile(
      bridgeConfig,
      JSON.stringify(
        {
          ...base,
          vars: {
            DIRECTORY_CELL: cell,
            QA_MEDIA: "1",
            QA_DEVICE: fixture.device,
            QA_MEDIA_ONE: media.objects[0].sha,
            QA_MEDIA_TWO: media.objects[1].sha,
          },
        },
        null,
        2,
      ),
    );
    const auth = JSON.parse(
      await runOwned(
        "auth-token",
        process.execPath,
        [cli, "auth", "token", "--json", "--config", bridgeConfig],
        undefined,
        { timeoutMs: 15_000, capture: true, env: { ...environment, WRANGLER_LOG: "log" } },
      ),
    );
    assert.ok(["oauth", "api_token"].includes(auth.type) && auth.token);
    credential = auth.token;
    before = await metadata();
    await save("before-private-metadata", before);
    phase = "actual-private-media-journey";
    await assertHelpers();
    await runOwned(
      "preview",
      process.execPath,
      [join(output, "preview.mjs")],
      JSON.stringify({
        repoRoot: root,
        output,
        fixture: fixturePath,
        mediaFixture: mediaPath,
        media: true,
        bridgeConfig,
        clientBin: frozenClient,
        secretHome,
      }),
    );
    assert.equal(
      JSON.parse(await readFile(join(output, "preview-cleanup.json"), "utf8")).previewStopped,
      true,
    );
    await assertHelpers();
  } catch {
    failure = {
      phase,
      detail: "Private media probe failed; foreign output/exception text withheld.",
    };
  } finally {
    if (credential && before) {
      try {
        const after = await metadata();
        await save("after-private-metadata", after);
        assert.deepEqual(after, before);
        cleanup.existingCloudMetadataUnchanged = true;
      } catch {
        cleanup.existingCloudMetadataUnchanged = false;
      }
    }
    if (secretParent) {
      try {
        await rm(secretParent, { recursive: true, force: false });
        cleanup.ownedKeysRemoved = true;
      } catch {
        cleanup.ownedKeysRemoved = false;
      }
    }
    const artifacts = [];
    const artifactReadFailures = [];
    for (const name of [
      "plan.json",
      "bridge.jsonc",
      "bridge.mjs",
      "preview.mjs",
      "directory-socket-client",
      "native-public-fixture.json",
      "native-public-media.json",
      "r2-object-cleanup.json",
      "before-private-metadata.json",
      "after-private-metadata.json",
      "remote-observations.json",
      "remote-client-observations.json",
      "preview-cleanup.json",
    ]) {
      try {
        artifacts.push(await artifact(join(output, name)));
      } catch (error) {
        if (error.code !== "ENOENT") artifactReadFailures.push({ name, code: "READ_FAILED" });
      }
    }
    if (artifactReadFailures.length)
      failure ??= { phase: "artifact-index", detail: "See sanitized artifactReadFailures" };
    await save("acceptance", {
      mode: "media",
      expectedSource: mediaSource,
      expectedVersion: mediaVersion,
      status: failure
        ? "FAILED_OWNER_R2_CLEANUP_STILL_REQUIRED"
        : "JOURNEY_PASS_PENDING_OWNER_R2_CLEANUP",
      scope:
        "Real DO media/R2 adapter in one hardcoded OWN QA cell only. No production fixed-cell/facade/public bucket acceptance.",
      failure,
      cleanup,
      sources,
      artifacts,
      artifactReadFailures,
    });
    console.log(
      JSON.stringify({ mode: "media", failure, cleanup, receipt: join(output, "acceptance.json") }),
    );
    if (failure || !cleanup.ownedKeysRemoved || !cleanup.existingCloudMetadataUnchanged)
      process.exitCode = 1;
  }
}

if (mediaMode) await runMediaMode();
else {
  const frozen = join(output, "candidate");
  await mkdir(join(frozen, "src"), { recursive: true });
  await mkdir(join(frozen, "generated/wasm"), { recursive: true });
  const sourceArtifacts = [];
  for (const name of ["directory-state.mjs", "directory-index.mjs"]) {
    const sourcePath = `infra/cloudflare/relay-state/src/${name}`;
    const bytes = execFileSync("git", ["show", `${candidate}:${sourcePath}`], {
      cwd: root,
      stdio: ["ignore", "pipe", "pipe"],
      maxBuffer: 2 * 1024 * 1024,
    });
    const path = join(frozen, "src", name);
    await writeFile(path, bytes);
    sourceArtifacts.push({ sourceCommit: candidate, sourcePath, ...(await artifact(path)) });
  }
  for (const [name, expected] of Object.entries(expectedWasm)) {
    const input = join(wasmDir, name);
    const observed = await artifact(input);
    assert.equal(observed.bytes, expected.bytes, `Sealed ${name} size`);
    assert.equal(observed.sha256, expected.sha256, `Sealed ${name} hash`);
    const path = join(frozen, "generated/wasm", name);
    await copyFile(input, path);
    sourceArtifacts.push({ ...(await artifact(path)), sealedInput: input });
  }
  const ownedSources = [
    "scripts/verify-cloudflare-directory-remote.mjs",
    "scripts/cloudflare-directory-remote-preview.mjs",
    "infra/cloudflare/relay-state/tests/directory-remote-bridge.mjs",
    "crates/roda-relay-admission/examples/directory-socket-client.rs",
    "infra/cloudflare/package.json",
    "infra/cloudflare/package-lock.json",
  ];
  for (const path of ownedSources) sourceArtifacts.push(await artifact(join(root, path)));
  await copyFile(
    join(root, "infra/cloudflare/relay-state/tests/directory-remote-bridge.mjs"),
    join(output, "bridge.mjs"),
  );
  await copyFile(
    join(root, "scripts/cloudflare-directory-remote-preview.mjs"),
    join(output, "preview.mjs"),
  );
  await writeFile(
    join(output, "cleanup-worker.mjs"),
    "export default {fetch(){return new Response('Not found',{status:404});}};\n",
  );
  const base = {
    name: worker,
    account_id: account,
    compatibility_date: "2026-10-18",
    workers_dev: false,
    preview_urls: false,
    routes: [],
  };
  const deployConfig = join(output, "deploy.jsonc");
  const cleanupConfig = join(output, "cleanup.jsonc");
  const bridgeConfig = join(output, "bridge.jsonc");
  await writeFile(
    deployConfig,
    JSON.stringify(
      {
        ...base,
        main: join(frozen, "src/directory-index.mjs"),
        vars: { DIRECTORY_CELL: cell, RELAY_NAME: "zoen-cloudflare-private" },
        durable_objects: { bindings: [{ name: "DIRECTORY", class_name: "DirectoryState" }] },
        migrations: [{ tag: "qa-init-v1", new_sqlite_classes: ["DirectoryState"] }],
      },
      null,
      2,
    ),
  );
  await writeFile(
    cleanupConfig,
    JSON.stringify(
      {
        ...base,
        main: join(output, "cleanup-worker.mjs"),
        migrations: [
          { tag: "qa-init-v1", new_sqlite_classes: ["DirectoryState"] },
          { tag: "qa-delete-v2", deleted_classes: ["DirectoryState"] },
        ],
      },
      null,
      2,
    ),
  );
  await save("plan", {
    mode: execute ? "explicit-execution" : "preparation-only-no-cloud-requests",
    candidate,
    account,
    worker,
    className: "DirectoryState",
    cell,
    exactRuntime: sourceArtifacts,
    provisioning:
      "ONE new Worker and ONE new SQLite namespace. workers_dev=false, preview_urls=false, routes=[]. No existing resource modifications.",
    preview:
      "Ephemeral authenticated remote preview with fixed cell and fixed public synthetic identity/device/Pong. Direct Cloudflare-edge sockets; preview credentials remain in memory.",
    nativeClient:
      "Separately build: cargo build --locked -p roda-relay-admission --example directory-socket-client. Supply --client-bin and --client-sha256. No Cargo in this runner.",
    checkpoints: [
      "private-preview-and-outer-http-refusal",
      "two-real-challenges-and-provisional-ready",
      "first-registration-and-stale-provisional-refusal",
      "normal-client-disconnect",
      "registered-reconnect-and-canonical-service-delivery",
      "self-unlink-final-send-fence-and-revoked-reconnect",
      "real-clock-untouched-hello-idle-close",
      "real-clock-auth-idle-close",
    ],
    cleanup:
      "Stop preview and owned clients, deploy deleted_classes migration, verify own namespace gone, DELETE own Worker, compare exact full before/after account inventories; remove own synthetic secrets in finally.",
    excluded:
      "No provider/OAuth login, public relay acceptance, forced DO hibernation, SQL inspection/injection, capacity256 matrix, MLS/Space/R2/DNS/secrets/account modifications. Local 32/34 gates unchanged.",
  });
  if (!execute) {
    console.log(
      JSON.stringify({
        prepared: true,
        cloudRequests: 0,
        candidate,
        worker,
        plan: join(output, "plan.json"),
      }),
    );
  } else {
    let credential;
    let before;
    let secretParent;
    let phase = "preflight";
    let failure = null;
    const cleanup = {};
    const deployment = {};
    const run = runOwned;
    async function api(path, method = "GET") {
      const response = await fetch(
        `https://api.cloudflare.com/client/v4/accounts/${account}${path}`,
        {
          method,
          headers: { Authorization: `Bearer ${credential}` },
          signal: AbortSignal.timeout(30_000),
        },
      );
      const body = await response.json();
      assert.equal(body.success, true, "Owned Cloudflare metadata operation failed");
      return body;
    }
    async function inventory() {
      const scripts = await api("/workers/scripts");
      const namespaces = [];
      for (let page = 1; ; page++) {
        const value = await api(`/workers/durable_objects/namespaces?per_page=100&page=${page}`);
        namespaces.push(...value.result);
        if (page >= (value.result_info?.total_pages ?? 1)) break;
        assert.ok(page < 100, "Bounded full account namespace inventory");
      }
      return {
        scripts: scripts.result
          .map((value) => ({ id: value.id, etag: value.etag }))
          .sort((a, b) => a.id.localeCompare(b.id)),
        namespaces: namespaces.sort((a, b) => a.id.localeCompare(b.id)),
      };
    }
    async function assertFrozen() {
      for (const original of sourceArtifacts) {
        const current = await artifact(original.path);
        assert.equal(current.sha256, original.sha256, "Reviewed source or sealed runtime drift");
      }
      assert.equal((await artifact(clientBin)).sha256, clientSha, "Native client binary drift");
    }
    try {
      await diskGuard();
      await assertFrozen();
      deployment.client = await artifact(clientBin);
      // Execute the checked binary's own copy so a concurrent relink cannot change
      // the signer between init and the actual server challenges.
      const frozenClient = join(output, "directory-socket-client");
      await copyFile(clientBin, frozenClient);
      assert.equal((await artifact(frozenClient)).sha256, clientSha);
      secretParent = await mkdtemp(join(tmpdir(), "zoen-directory-owned-client-"));
      const secretHome = join(secretParent, "keys");
      const fixture = JSON.parse(
        await run(
          "native-init",
          frozenClient,
          [],
          JSON.stringify({ op: "init", home: secretHome }),
        ),
      );
      assert.equal(fixture.schema, 1);
      for (const key of ["identity", "device"]) assert.match(fixture[key], /^[a-f0-9]{64}$/);
      const fixturePath = join(output, "native-public-fixture.json");
      await writeFile(fixturePath, `${JSON.stringify(fixture, null, 2)}\n`);
      await writeFile(
        bridgeConfig,
        JSON.stringify(
          {
            ...base,
            name: `${worker}-preview`,
            main: join(output, "bridge.mjs"),
            vars: {
              DIRECTORY_CELL: cell,
              QA_IDENTITY: fixture.identity,
              QA_DEVICE: fixture.device,
              QA_PONG: fixture.pong_base64,
            },
            durable_objects: {
              bindings: [{ name: "DIRECTORY", class_name: "DirectoryState", script_name: worker }],
            },
            services: [{ binding: "PRIVATE", service: worker }],
          },
          null,
          2,
        ),
      );
      const auth = JSON.parse(
        await run(
          "auth-token",
          process.execPath,
          [cli, "auth", "token", "--json", "--config", deployConfig],
          undefined,
          { timeoutMs: 15_000, capture: true, env: { ...environment, WRANGLER_LOG: "log" } },
        ),
      );
      assert.ok(["oauth", "api_token"].includes(auth.type) && auth.token);
      credential = auth.token; // Memory only: never serialized, printed or passed in argv.
      before = await inventory();
      await save("before-inventory", before);
      assert.ok(!before.scripts.some((value) => value.id === worker));
      await save("owned-resources", {
        candidate,
        account,
        worker,
        cell,
        className: "DirectoryState",
      });
      phase = "deploy-one-owned-private-worker";
      await assertFrozen();
      await run("deploy", process.execPath, [cli, "deploy", "--config", deployConfig]);
      const provisioned = await inventory();
      await save("provisioned-inventory", provisioned);
      const added = provisioned.namespaces.filter(
        (value) => !before.namespaces.some((old) => old.id === value.id),
      );
      assert.equal(added.length, 1);
      assert.equal(added[0].script, worker);
      assert.equal(added[0].class, "DirectoryState");
      assert.equal(added[0].use_sqlite, true);
      deployment.namespace = added[0];
      assert.deepEqual(
        provisioned.scripts.filter((value) => value.id !== worker),
        before.scripts,
      );
      assert.deepEqual(
        provisioned.namespaces.filter((value) => value.id !== added[0].id),
        before.namespaces,
      );
      const endpoint = `/workers/services/${worker}/environments/production`;
      const bindings = (await api(`${endpoint}/bindings`)).result;
      const routes = (await api(`${endpoint}/routes?show_zonename=true`)).result;
      const subdomain = (await api(`${endpoint}/subdomain`)).result;
      const versions = (await api(`/workers/scripts/${worker}/deployments`)).result;
      const objectBindings = (Array.isArray(bindings) ? bindings : bindings.bindings).filter(
        (value) => value.type === "durable_object_namespace",
      );
      assert.equal(objectBindings.length, 1);
      assert.equal(objectBindings[0].namespace_id, added[0].id);
      assert.deepEqual(routes, []);
      assert.equal(subdomain.enabled, false);
      assert.equal(subdomain.previews_enabled, false);
      await save("private-provisioning", {
        worker,
        namespace: added[0],
        bindings,
        routes,
        subdomain,
        versions,
      });
      phase = "actual-authenticated-edge-directory-journey";
      await assertFrozen();
      await run(
        "preview",
        process.execPath,
        [join(output, "preview.mjs")],
        JSON.stringify({
          repoRoot: root,
          output,
          fixture: fixturePath,
          bridgeConfig,
          clientBin: frozenClient,
          secretHome,
        }),
      );
      assert.equal(
        JSON.parse(await readFile(join(output, "preview-cleanup.json"), "utf8")).previewStopped,
        true,
      );
      await assertFrozen();
    } catch {
      failure = {
        phase,
        detail:
          "Private Directory remote probe failed; foreign error/output withheld to protect credentials.",
      };
    } finally {
      if (credential && before) {
        try {
          const state = await inventory();
          if (
            state.scripts.some((value) => value.id === worker) ||
            state.namespaces.some((value) => value.script === worker)
          ) {
            await run("delete-owned-class", process.execPath, [
              cli,
              "deploy",
              "--config",
              cleanupConfig,
            ]);
            const noClass = await inventory();
            assert.ok(!noClass.namespaces.some((value) => value.script === worker));
            cleanup.namespaceAbsent = true;
            await save("after-class-deletion-inventory", noClass);
            await api(`/workers/scripts/${worker}`, "DELETE");
            cleanup.workerDeleted = true;
          }
          const after = await inventory();
          await save("after-inventory", after);
          assert.deepEqual(after, before, "Exact account inventory must be restored");
          cleanup.exactAccountInventoryRestored = true;
        } catch {
          cleanup.failure = "Owned resource cleanup failed; inspect safe metadata receipts.";
        }
      }
      if (secretParent) {
        try {
          await rm(secretParent, { recursive: true, force: false });
          cleanup.ownedSecretsRemoved = true;
        } catch {
          cleanup.ownedSecretsRemoved = false;
        }
      }
      const artifacts = [];
      const artifactReadFailures = [];
      for (const name of [
        "plan.json",
        "native-public-fixture.json",
        "deploy.jsonc",
        "cleanup.jsonc",
        "bridge.jsonc",
        "bridge.mjs",
        "preview.mjs",
        "directory-socket-client",
        "cleanup-worker.mjs",
        "owned-resources.json",
        "before-inventory.json",
        "private-provisioning.json",
        "remote-observations.json",
        "remote-client-observations.json",
        "preview-cleanup.json",
        "after-inventory.json",
      ]) {
        try {
          artifacts.push(await artifact(join(output, name)));
        } catch (error) {
          if (error.code !== "ENOENT")
            artifactReadFailures.push({
              name,
              code:
                typeof error.code === "string" && /^[A-Z0-9_]{1,40}$/.test(error.code)
                  ? error.code
                  : "UNKNOWN",
              detail: "Evidence artifact could not be read.",
            });
        }
      }
      if (artifactReadFailures.length)
        failure ??= {
          phase: "artifact-index",
          detail: "Evidence artifact could not be read; inspect sanitized artifactReadFailures.",
        };
      await save("acceptance", {
        candidate,
        worker,
        scope:
          "Bounded private remote Directory socket journey only; local 32/34 pinned-workerd gates unchanged. No public relay/provider login/forced remote hibernation claim.",
        failure,
        cleanup,
        deployment,
        sourceArtifacts,
        artifacts,
        artifactReadFailures,
      });
      console.log(
        JSON.stringify({
          candidate,
          worker,
          failure,
          cleanup,
          receipt: join(output, "acceptance.json"),
        }),
      );
      if (failure || !cleanup.exactAccountInventoryRestored || !cleanup.ownedSecretsRemoved)
        process.exitCode = 1;
    }
  }
}
