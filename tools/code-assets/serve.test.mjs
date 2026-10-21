import assert from "node:assert/strict";
import { after, before, test } from "node:test";
import { createHash } from "node:crypto";
import { execFileSync, spawn } from "node:child_process";
import {
  chmod,
  copyFile,
  mkdir,
  mkdtemp,
  readFile,
  realpath,
  rm,
  symlink,
  writeFile,
} from "node:fs/promises";
import { request, Agent } from "node:https";
import { createServer, connect } from "node:net";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { startCodeAssetService } from "./serve.mjs";

const hash = (bytes) => createHash("sha256").update(bytes).digest("hex");
const revision = "a".repeat(40);
const moduleBytes = Buffer.from("export const zoenHostStart = () => {};\n");
const wasmBytes = Buffer.from([0, 97, 115, 109, 1, 0, 0, 0]);
let scratch;
let cert;
let key;
let ca;

before(async () => {
  scratch = await realpath(await mkdtemp(join(tmpdir(), "zoen-code-assets-test-")));
  cert = join(scratch, "qa-only.crt");
  key = join(scratch, "qa-only.key");
  const config = join(scratch, "qa-only.cnf");
  await writeFile(
    config,
    "[req]\nprompt=no\ndistinguished_name=dn\nx509_extensions=server\n[dn]\nCN=localhost\n[server]\nsubjectAltName=DNS:localhost,IP:127.0.0.1\nbasicConstraints=critical,CA:FALSE\nkeyUsage=critical,digitalSignature,keyEncipherment\nextendedKeyUsage=serverAuth\n",
  );
  // Test-only ephemeral identity. No OS trust, process-wide CA setting or persistent service.
  execFileSync(
    "/usr/bin/openssl",
    [
      "req",
      "-x509",
      "-newkey",
      "rsa:2048",
      "-nodes",
      "-days",
      "1",
      "-config",
      config,
      "-keyout",
      key,
      "-out",
      cert,
    ],
    { stdio: "ignore" },
  );
  await chmod(key, 0o600);
  ca = await readFile(cert);
});

after(async () => {
  await rm(scratch, { recursive: true, force: true });
});

async function freePort() {
  const server = createServer();
  await new Promise((done) => server.listen(0, "127.0.0.1", done));
  const { port } = server.address();
  await new Promise((done) => server.close(done));
  return port;
}

async function fixture(t) {
  const folder = await mkdtemp(join(scratch, "package-"));
  const root = join(folder, "assets");
  const directory = join(root, "deckcraft", revision);
  await mkdir(directory, { recursive: true });
  const port = await freePort();
  const prefix = `https://localhost:${port}/deckcraft/${revision}/`;
  const runtime = {
    schema: 1,
    engine: "deckcraft",
    revision,
    abi: "zoen-host-v1",
    module: {
      url: prefix + "runtime.js",
      bytes: moduleBytes.length,
      sha256: hash(moduleBytes),
      mime: "text/javascript",
    },
    wasm: {
      url: prefix + "runtime.wasm",
      bytes: wasmBytes.length,
      sha256: hash(wasmBytes),
      mime: "application/wasm",
    },
  };
  const packagePath = join(folder, "runtime-package.json");
  await writeFile(join(directory, "runtime.js"), moduleBytes);
  await writeFile(join(directory, "runtime.wasm"), wasmBytes);
  const options = { package: packagePath, root, cert, key, profile: join(folder, "profile"), port };
  const save = async () => {
    const bytes = JSON.stringify(runtime, null, 2) + "\n";
    await writeFile(packagePath, bytes);
    options.packageSha256 = hash(bytes);
  };
  await save();
  let service;
  t.after(async () => {
    await service?.close();
  });
  return {
    options,
    runtime,
    directory,
    save,
    start: async () => {
      service = await startCodeAssetService(options);
      return service;
    },
  };
}

async function fetchAsset(options, path, extra = {}) {
  return new Promise((done, reject) => {
    const req = request(
      {
        hostname: "127.0.0.1",
        port: options.port,
        servername: "localhost",
        path,
        method: "GET",
        ca,
        rejectUnauthorized: true,
        agent: false,
        headers: { Host: `localhost:${options.port}` },
        ...extra,
      },
      (response) => {
        const chunks = [];
        response.on("data", (chunk) => chunks.push(chunk));
        response.on("error", reject);
        response.on("end", () =>
          done({
            status: response.statusCode,
            headers: response.headers,
            bytes: Buffer.concat(chunks),
          }),
        );
      },
    );
    req.on("error", reject);
    req.end();
  });
}

test("real TLS serves only the admitted bytes with exact MIME, CORS and HEAD semantics", async (t) => {
  const f = await fixture(t);
  const service = await f.start();
  for (const [asset, expected] of [
    [f.runtime.module, moduleBytes],
    [f.runtime.wasm, wasmBytes],
  ]) {
    const result = await fetchAsset(f.options, new URL(asset.url).pathname);
    assert.equal(result.status, 200);
    assert.deepEqual(result.bytes, expected);
    assert.equal(result.headers["content-type"], asset.mime);
    assert.equal(Number(result.headers["content-length"]), expected.length);
    assert.equal(result.headers["access-control-allow-origin"], "*");
    assert.equal(result.headers["x-content-type-options"], "nosniff");
    assert.equal(result.headers["cache-control"], "no-store, no-transform");
    assert.equal(result.headers["access-control-allow-credentials"], undefined);
    const head = await fetchAsset(f.options, new URL(asset.url).pathname, { method: "HEAD" });
    assert.equal(head.status, 200);
    assert.equal(head.bytes.length, 0);
    assert.equal(Number(head.headers["content-length"]), expected.length);
  }
  const receipt = JSON.parse(await readFile(service.receiptPath));
  assert.equal(receipt.status, "ready");
  assert.equal(receipt.packageSha256, f.options.packageSha256);
  assert.equal(receipt.assets.length, 2);
  assert.ok(!JSON.stringify(receipt).includes("PRIVATE KEY"));
});

test("raw aliases, traversal, documents, queries and foreign Host are never routes", async (t) => {
  const f = await fixture(t);
  await writeFile(join(f.options.root, "secret-document.txt"), "must never serve");
  await f.start();
  const path = new URL(f.runtime.module.url).pathname;
  for (const candidate of [
    "/",
    "/runtime-package.json",
    "/secret-document.txt",
    path + "?cache=1",
    path + "#fragment",
    path.replace("runtime.js", "%72untime.js"),
    path.replace("/deckcraft/", "//deckcraft/"),
    `/deckcraft/../deckcraft/${revision}/runtime.js`,
    `/deckcraft/%2e%2e/deckcraft/${revision}/runtime.js`,
  ]) {
    const result = await fetchAsset(f.options, candidate);
    assert.equal(result.status, 404, candidate);
    assert.equal(result.bytes.length, 0);
    assert.equal(result.headers.location, undefined);
  }
  assert.equal(
    (await fetchAsset(f.options, path, { headers: { Host: "foreign.invalid" } })).status,
    404,
  );
  assert.equal((await fetchAsset(f.options, path, { method: "POST" })).status, 405);
  assert.equal(
    (
      await fetchAsset(f.options, path, {
        method: "GET",
        headers: {
          Host: `localhost:${f.options.port}`,
          "Content-Length": "1",
        },
      })
    ).status,
    405,
  );
});

test("anonymous TLS still requires explicit client trust", async (t) => {
  const f = await fixture(t);
  await f.start();
  await assert.rejects(
    fetchAsset(f.options, new URL(f.runtime.module.url).pathname, { ca: undefined }),
    (error) => ["DEPTH_ZERO_SELF_SIGNED_CERT", "SELF_SIGNED_CERT_IN_CHAIN"].includes(error.code),
  );
});

test("source mutation after admission cannot alter served bytes", async (t) => {
  const f = await fixture(t);
  await f.start();
  await writeFile(join(f.directory, "runtime.js"), "foreign bytes after startup");
  assert.deepEqual(
    (await fetchAsset(f.options, new URL(f.runtime.module.url).pathname)).bytes,
    moduleBytes,
  );
});

test("foreign package hash, asset hash and asset length fail before any lease", async (t) => {
  for (const variant of ["package", "hash", "length"]) {
    const f = await fixture(t);
    if (variant === "package") f.options.packageSha256 = "0".repeat(64);
    else {
      if (variant === "hash") f.runtime.wasm.sha256 = "0".repeat(64);
      else f.runtime.wasm.bytes += 1;
      await f.save();
    }
    await assert.rejects(f.start(), /mismatch/);
    await assert.rejects(readFile(join(f.options.profile, "active.json")), { code: "ENOENT" });
  }
});

test("noncanonical network package URLs and unexpected package fields are rejected", async (t) => {
  for (const change of [
    (f) => {
      f.runtime.module.url = f.runtime.module.url.replace("localhost", "example.com");
    },
    (f) => {
      f.runtime.module.url += "?key=not-admitted";
    },
    (f) => {
      f.runtime.module.url = f.runtime.module.url.replace(
        "/deckcraft/",
        "/deckcraft/../deckcraft/",
      );
    },
    (f) => {
      f.runtime.module.url = f.runtime.module.url.replace("runtime.js", "%72untime.js");
    },
    (f) => {
      f.runtime.module.bytes = 512 * 1024 + 1;
    },
    (f) => {
      f.runtime.document = "not a code asset";
    },
  ]) {
    const f = await fixture(t);
    change(f);
    await f.save();
    await assert.rejects(f.start(), /Invalid/);
  }
});

test("symlink and group-writable assets cannot be admitted", async (t) => {
  const symlinkFixture = await fixture(t);
  const asset = join(symlinkFixture.directory, "runtime.js");
  await rm(asset);
  await symlink(symlinkFixture.options.package, asset);
  await assert.rejects(symlinkFixture.start(), /symlinks/);
  const writable = await fixture(t);
  await chmod(join(writable.directory, "runtime.js"), 0o666);
  await assert.rejects(writable.start(), /safe permissions/);
});

test("private key protection and certificate pairing are mandatory", async (t) => {
  const f = await fixture(t);
  f.options.key = join(f.directory, "unsafe.key");
  await copyFile(key, f.options.key);
  await chmod(f.options.key, 0o644);
  await assert.rejects(f.start(), /safe permissions/);
  await chmod(f.options.key, 0o600);
  execFileSync("/usr/bin/openssl", ["genrsa", "-out", f.options.key, "2048"], { stdio: "ignore" });
  await assert.rejects(f.start(), /mismatch/);
});

test("profile cannot be reused for a different tuple and active lease is never adopted", async (t) => {
  const f = await fixture(t);
  const service = await f.start();
  await assert.rejects(startCodeAssetService(f.options), { code: "EEXIST" });
  await service.close();
  await writeFile(
    join(f.options.profile, "active.json"),
    JSON.stringify({ pid: 99999999, runId: "foreign" }),
    { mode: 0o600 },
  );
  await assert.rejects(startCodeAssetService(f.options), { code: "EEXIST" });
  assert.equal(JSON.parse(await readFile(join(f.options.profile, "active.json"))).runId, "foreign");
  await rm(join(f.options.profile, "active.json"));
  f.runtime.module.sha256 = hash(Buffer.from("replacement"));
  f.runtime.module.bytes = Buffer.byteLength("replacement");
  await writeFile(join(f.directory, "runtime.js"), "replacement");
  await f.save();
  await assert.rejects(startCodeAssetService(f.options), /changed service profile/);
});

test("occupied port fails without closing foreign listener and releases only own startup lease", async (t) => {
  const f = await fixture(t);
  const foreign = createServer((socket) => socket.end("foreign"));
  t.after(() => new Promise((done) => foreign.close(done)));
  await new Promise((done) => foreign.listen(f.options.port, "127.0.0.1", done));
  await assert.rejects(f.start(), { code: "EADDRINUSE" });
  assert.equal(foreign.listening, true);
  await assert.rejects(readFile(join(f.options.profile, "active.json")), { code: "ENOENT" });
  assert.equal(
    JSON.parse(await readFile(join(f.options.profile, "receipt.json"))).status,
    "failed",
  );
});

test("close destroys idle and unfinished TLS sockets, releases lease and permits explicit restart", async (t) => {
  const f = await fixture(t);
  const service = await f.start();
  const agent = new Agent({ keepAlive: true });
  t.after(() => agent.destroy());
  await fetchAsset(f.options, new URL(f.runtime.module.url).pathname, { agent });
  const idle = connect(f.options.port, "127.0.0.1");
  await new Promise((done, reject) => {
    idle.once("connect", done);
    idle.once("error", reject);
  });
  const closed = new Promise((done) => idle.once("close", done));
  await Promise.all([service.close(), service.close()]);
  await closed;
  assert.equal(JSON.parse(await readFile(service.receiptPath)).status, "closed");
  await assert.rejects(readFile(join(f.options.profile, "active.json")), { code: "ENOENT" });
  const restarted = await startCodeAssetService(f.options);
  await restarted.close();
});

test("shutdown cannot remove or overwrite a replaced foreign lease", async (t) => {
  const f = await fixture(t);
  const service = await f.start();
  const foreign = { pid: 1234, runId: "foreign-owner" };
  await writeFile(join(f.options.profile, "active.json"), JSON.stringify(foreign), { mode: 0o600 });
  await service.close();
  assert.deepEqual(JSON.parse(await readFile(join(f.options.profile, "active.json"))), foreign);
  assert.equal(JSON.parse(await readFile(service.receiptPath)).status, "ready");
});

test("foreground CLI SIGTERM closes its real listener and records a closed receipt", async (t) => {
  const f = await fixture(t);
  const args = [
    "--package",
    f.options.package,
    "--package-sha256",
    f.options.packageSha256,
    "--root",
    f.options.root,
    "--cert",
    cert,
    "--key",
    key,
    "--profile",
    f.options.profile,
    "--port",
    String(f.options.port),
  ];
  const child = spawn(
    process.execPath,
    [new URL("./serve.mjs", import.meta.url).pathname, ...args],
    { stdio: ["ignore", "pipe", "pipe"] },
  );
  const exited = new Promise((done, reject) => {
    child.once("error", reject);
    child.once("exit", (code, signal) => done({ code, signal }));
  });
  t.after(async () => {
    if (child.exitCode === null && child.signalCode === null) child.kill("SIGKILL");
    await exited;
  });
  const line = await new Promise((done, reject) => {
    let text = "";
    const timer = setTimeout(() => reject(new Error("Foreground startup timed out")), 10000);
    child.stdout.on("data", (chunk) => {
      text += chunk;
      if (text.includes("\n")) {
        clearTimeout(timer);
        done(text.trim());
      }
    });
    child.once("exit", () => {
      clearTimeout(timer);
      if (!text) reject(new Error("Foreground startup refused"));
    });
  });
  const receipt = JSON.parse(line);
  assert.equal(receipt.origin, `https://localhost:${f.options.port}`);
  assert.equal((await fetchAsset(f.options, new URL(f.runtime.module.url).pathname)).status, 200);
  child.kill("SIGTERM");
  assert.deepEqual(await exited, { code: 0, signal: null });
  assert.equal(JSON.parse(await readFile(receipt.receipt)).status, "closed");
  await assert.rejects(readFile(join(f.options.profile, "active.json")), { code: "ENOENT" });
  await assert.rejects(fetchAsset(f.options, new URL(f.runtime.module.url).pathname), {
    code: "ECONNREFUSED",
  });
});
