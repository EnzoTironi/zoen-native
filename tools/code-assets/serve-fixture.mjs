import { after, before } from "node:test";
import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import {
  chmod,
  mkdir,
  mkdtemp,
  readFile,
  realpath,
  rm,
  writeFile,
} from "node:fs/promises";
import { request } from "node:https";
import { createServer } from "node:net";
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

async function retainedFixture(f) {
  const folder = await mkdtemp(join(scratch, "retained-"));
  const root = join(folder, "assets");
  const directory = join(root, "deckcraft", revision);
  await mkdir(directory, { recursive: true });
  const module = Buffer.from("export const previousRuntime = true;\n");
  const wasm = Buffer.concat([wasmBytes, Buffer.from([0, 1, 0])]);
  const runtime = structuredClone(f.runtime);
  runtime.module = {
    ...runtime.module,
    url: runtime.module.url.replace("runtime.js", "previous.js"),
    bytes: module.length,
    sha256: hash(module),
  };
  runtime.wasm = {
    ...runtime.wasm,
    url: runtime.wasm.url.replace("runtime.wasm", "previous.wasm"),
    bytes: wasm.length,
    sha256: hash(wasm),
  };
  const packagePath = join(folder, "previous-package.json");
  await writeFile(join(directory, "previous.js"), module);
  await writeFile(join(directory, "previous.wasm"), wasm);
  const save = async () => {
    const bytes = JSON.stringify(runtime, null, 2) + "\n";
    await writeFile(packagePath, bytes);
    Object.assign(f.options, {
      retainedPackage: packagePath,
      retainedPackageSha256: hash(bytes),
      retainedRoot: root,
    });
  };
  await save();
  return { runtime, root, directory, packagePath, module, wasm, save };
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


export { fixture, retainedFixture, fetchAsset, freePort, hash, revision, moduleBytes, wasmBytes, scratch, cert, key, ca };
