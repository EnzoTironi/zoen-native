import { constants } from "node:fs";
import { lstat, mkdir, open, realpath, rename, unlink } from "node:fs/promises";
import { createHash, createPrivateKey, randomUUID, X509Certificate } from "node:crypto";
import { createServer } from "node:https";
import { isAbsolute, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const HASH = /^[a-f0-9]{64}$/;
const sha256 = (bytes) => createHash("sha256").update(bytes).digest("hex");
const fail = (message) => {
  throw new Error(message);
};
const uid = process.getuid?.();
const LIMITS = { module: 512 * 1024, wasm: 48 * 1024 * 1024 };

function absolute(value, label) {
  if (typeof value !== "string" || !isAbsolute(value)) fail(`${label} must be an absolute path`);
  return resolve(value);
}

function owned(stat, label, privateMode = false) {
  if (uid === undefined || stat.uid !== uid || stat.mode & (privateMode ? 0o077 : 0o022))
    fail(`${label} must be owned by the current user with safe permissions`);
}

async function readBounded(filename, limit, privateMode = false) {
  const file = await open(filename, constants.O_RDONLY | constants.O_NOFOLLOW);
  try {
    const stat = await file.stat();
    owned(stat, filename, privateMode);
    if (!stat.isFile() || stat.size < 1 || stat.size > limit)
      fail(`Invalid bounded file: ${filename}`);
    const bytes = Buffer.alloc(stat.size + 1);
    let count = 0;
    while (count < bytes.length) {
      const { bytesRead } = await file.read(bytes, count, bytes.length - count, null);
      if (!bytesRead) break;
      count += bytesRead;
    }
    if (count !== stat.size) fail(`File changed while reading: ${filename}`);
    return bytes.subarray(0, count);
  } finally {
    await file.close();
  }
}

function record(value, keys) {
  if (
    !value ||
    typeof value !== "object" ||
    Array.isArray(value) ||
    Object.keys(value).sort().join(",") !== [...keys].sort().join(",")
  )
    fail("Invalid runtime package shape");
  return value;
}

/** Matches the existing runtime-package.ts contract at the serving boundary. */
function parsePackage(value, port) {
  const input = record(value, ["schema", "engine", "revision", "abi", "module", "wasm"]);
  if (
    input.schema !== 1 ||
    input.engine !== "deckcraft" ||
    input.abi !== "zoen-host-v1" ||
    typeof input.revision !== "string" ||
    !/^[a-f0-9]{40}$/.test(input.revision)
  )
    fail("Invalid runtime package identity");
  const assets = ["module", "wasm"].map((kind) => {
    const item = record(input[kind], ["url", "bytes", "sha256", "mime"]);
    if (typeof item.url !== "string" || item.url.length > 2048) fail("Invalid asset URL");
    const url = new URL(item.url);
    const mime = kind === "module" ? "text/javascript" : "application/wasm";
    if (
      url.protocol !== "https:" ||
      !["localhost", "127.0.0.1"].includes(url.hostname) ||
      Number(url.port || 443) !== port ||
      url.username ||
      url.password ||
      url.search ||
      url.hash ||
      url.href !== item.url ||
      !/^\/[a-zA-Z0-9/_.,-]+$/.test(url.pathname) ||
      url.pathname.includes("//") ||
      url.pathname.split("/").some((part) => part === "." || part === "..") ||
      !url.pathname.endsWith(kind === "module" ? ".js" : ".wasm") ||
      item.mime !== mime ||
      !Number.isSafeInteger(item.bytes) ||
      item.bytes < (kind === "wasm" ? 8 : 1) ||
      item.bytes > LIMITS[kind] ||
      typeof item.sha256 !== "string" ||
      !HASH.test(item.sha256)
    )
      fail("Invalid loopback code asset");
    return { ...item, url, kind };
  });
  const directory = (url) => url.pathname.slice(0, url.pathname.lastIndexOf("/") + 1);
  if (
    assets[0].url.origin !== assets[1].url.origin ||
    directory(assets[0].url) !== directory(assets[1].url) ||
    !directory(assets[0].url).endsWith(`/${input.revision}/`)
  )
    fail("Invalid shared runtime directory");
  return {
    assets,
    origin: assets[0].url.origin,
    host: assets[0].url.host,
    revision: input.revision,
  };
}

async function jsonFile(filename, value, exclusive = false) {
  const bytes = JSON.stringify(value, null, 2) + "\n";
  if (Buffer.byteLength(bytes) > 16384) fail("Receipt exceeds bound");
  if (exclusive) {
    const file = await open(filename, "wx", 0o600);
    try {
      await file.writeFile(bytes);
    } finally {
      await file.close();
    }
    return;
  }
  const temporary = `${filename}.${randomUUID()}.new`;
  try {
    await jsonFile(temporary, value, true);
    await rename(temporary, filename);
  } finally {
    await unlink(temporary).catch((error) => {
      if (error.code !== "ENOENT") throw error;
    });
  }
}

async function leaseMatches(filename, lease) {
  try {
    return (
      JSON.stringify(JSON.parse(await readBounded(filename, 1024, true))) === JSON.stringify(lease)
    );
  } catch {
    return false;
  }
}

/** Foreground-only, immutable anonymous code serving; never provisions trust or changes pins. */
export async function startCodeAssetService(options) {
  const port = options.port ?? 9446;
  if (!Number.isInteger(port) || port < 1024 || port > 65535) fail("Port must be 1024..65535");
  const packageSha256 = options.packageSha256;
  if (typeof packageSha256 !== "string" || !HASH.test(packageSha256))
    fail("Explicit package SHA-256 required");
  const packagePath = absolute(options.package, "Package");
  const root = absolute(options.root, "Asset root");
  const profile = absolute(options.profile, "Profile");
  const certPath = absolute(options.cert, "Certificate");
  const keyPath = absolute(options.key, "Private key");
  const retainedCount = [
    options.retainedPackage,
    options.retainedPackageSha256,
    options.retainedRoot,
  ].filter((value) => value !== undefined).length;
  if (retainedCount !== 0 && retainedCount !== 3) fail("Complete retained package tuple required");
  // At most one explicit retained tuple. This is not a package registry or filesystem router.
  const packages = [{ root, packagePath, packageSha256 }];
  if (retainedCount) {
    if (
      typeof options.retainedPackageSha256 !== "string" ||
      !HASH.test(options.retainedPackageSha256)
    )
      fail("Explicit retained package SHA-256 required");
    packages.push({
      root: absolute(options.retainedRoot, "Retained asset root"),
      packagePath: absolute(options.retainedPackage, "Retained package"),
      packageSha256: options.retainedPackageSha256,
    });
  }
  // Same immutable in-memory allowlist, with all packages verified before any lease or listener.
  const only = new Map();
  for (const packageInput of packages) {
    const { root, packagePath, packageSha256 } = packageInput;
    if ((await realpath(root)) !== root) fail("Asset root must be canonical");
    const rootStat = await lstat(root);
    owned(rootStat, "Asset root");
    if (!rootStat.isDirectory()) fail("Asset root must be a directory");
    const packageBytes = await readBounded(packagePath, 64 * 1024);
    if (sha256(packageBytes) !== packageSha256) fail("Package SHA-256 mismatch");
    const runtime = parsePackage(JSON.parse(packageBytes.toString("utf8")), port);
    packageInput.runtime = runtime;
    if (runtime.origin !== packages[0].runtime.origin) fail("Retained package origin mismatch");
    for (const asset of runtime.assets) {
      if (only.has(asset.url.pathname)) fail("Duplicate code asset route");
      const filename = join(root, asset.url.pathname.slice(1));
      if ((await realpath(filename)) !== filename) fail("Asset path must not contain symlinks");
      const bytes = await readBounded(filename, LIMITS[asset.kind]);
      if (bytes.length !== asset.bytes || sha256(bytes) !== asset.sha256)
        fail("Code asset hash or length mismatch");
      if (
        asset.kind === "wasm" &&
        !bytes.subarray(0, 8).equals(Buffer.from([0, 97, 115, 109, 1, 0, 0, 0]))
      )
        fail("Invalid WASM header");
      if (asset.kind === "module") new TextDecoder("utf-8", { fatal: true }).decode(bytes);
      only.set(asset.url.pathname, { bytes, mime: asset.mime });
    }
  }
  const runtime = packages[0].runtime;
  const cert = await readBounded(certPath, 256 * 1024);
  const key = await readBounded(keyPath, 64 * 1024, true);
  const leaf = new X509Certificate(cert);
  const privateKey = createPrivateKey(key);
  if (
    !leaf.checkPrivateKey(privateKey) ||
    Date.parse(leaf.validFrom) > Date.now() ||
    Date.parse(leaf.validTo) <= Date.now() ||
    !(runtime.host.startsWith("localhost:")
      ? leaf.checkHost("localhost")
      : leaf.checkIP("127.0.0.1"))
  )
    fail("Certificate identity, validity or private key mismatch");
  const certSha256 = sha256(leaf.raw);
  const contract = {
    schema: 1,
    purpose: "zoen-code-assets",
    origin: runtime.origin,
    root,
    packagePath,
    packageSha256,
    certSha256,
    ...(retainedCount
      ? {
          retainedPackage: {
            root: packages[1].root,
            packagePath: packages[1].packagePath,
            packageSha256: packages[1].packageSha256,
          },
        }
      : {}),
  };
  // mkdir is intentionally nonrecursive: the caller owns/provisions the private parent.
  await mkdir(profile, { mode: 0o700 }).catch((error) => {
    if (error.code !== "EEXIST") throw error;
  });
  if ((await realpath(profile)) !== profile) fail("Profile must be canonical");
  const profileStat = await lstat(profile);
  owned(profileStat, "Profile", true);
  if (!profileStat.isDirectory()) fail("Profile must be a directory");
  const ownerPath = join(profile, "owner.json");
  try {
    await jsonFile(ownerPath, contract, true);
  } catch (error) {
    if (error.code !== "EEXIST") throw error;
    const owner = JSON.parse(await readBounded(ownerPath, 16384, true));
    if (JSON.stringify(owner) !== JSON.stringify(contract))
      fail("Foreign or changed service profile");
  }
  const activePath = join(profile, "active.json");
  const lease = { pid: process.pid, runId: randomUUID() };
  await jsonFile(activePath, lease, true); // No stale-lock adoption, process signalling or foreign cleanup.
  const receiptPath = join(profile, "receipt.json");
  const receipt = {
    ...contract,
    ...lease,
    startedAt: new Date().toISOString(),
    assets: packages.flatMap(({ runtime }) =>
      runtime.assets.map((asset) => ({
        path: asset.url.pathname,
        bytes: asset.bytes,
        sha256: asset.sha256,
        mime: asset.mime,
      })),
    ),
  };
  const sockets = new Set();
  let server;
  let closing;
  const finish = async (status) => {
    if (await leaseMatches(activePath, lease)) {
      await jsonFile(receiptPath, { ...receipt, status, stoppedAt: new Date().toISOString() });
      if (await leaseMatches(activePath, lease)) await unlink(activePath);
    }
  };
  const close = () =>
    (closing ??= (async () => {
      if (server) {
        const stopped = new Promise((done) => server.close(() => done()));
        for (const socket of sockets) socket.destroy();
        await stopped;
      }
      await finish("closed");
    })());
  try {
    server = createServer(
      { cert, key, minVersion: "TLSv1.2", maxHeaderSize: 8192 },
      (request, response) => {
        const asset = only.get(request.url);
        if (request.headers.host?.toLowerCase() !== runtime.host || !asset) {
          response.writeHead(404, { "Content-Length": "0", "Cache-Control": "no-store" });
          response.end();
        } else if (
          !["GET", "HEAD"].includes(request.method) ||
          request.headers["transfer-encoding"] ||
          Number(request.headers["content-length"] ?? 0) !== 0
        ) {
          response.writeHead(405, {
            Allow: "GET, HEAD",
            "Content-Length": "0",
            "Cache-Control": "no-store",
          });
          response.end();
        } else {
          response.writeHead(200, {
            "Content-Type": asset.mime,
            "Content-Length": asset.bytes.length,
            "Cache-Control": "no-store, no-transform",
            "Access-Control-Allow-Origin": "*",
            "X-Content-Type-Options": "nosniff",
          });
          response.end(request.method === "HEAD" ? undefined : asset.bytes);
        }
      },
    );
    server.maxConnections = 32;
    server.headersTimeout = 5000;
    server.requestTimeout = 10000;
    server.keepAliveTimeout = 1000;
    server.on("connection", (socket) => {
      sockets.add(socket);
      socket.setTimeout(10000, () => socket.destroy());
      socket.on("close", () => sockets.delete(socket));
    });
    await new Promise((done, reject) => {
      server.once("error", reject);
      server.listen(port, "127.0.0.1", () => {
        server.off("error", reject);
        done();
      });
    });
    await jsonFile(receiptPath, { ...receipt, status: "ready" });
    return Object.freeze({ origin: runtime.origin, port, receiptPath, close });
  } catch (error) {
    if (server) {
      for (const socket of sockets) socket.destroy();
      await new Promise((done) => server.close(() => done()));
    }
    await finish("failed");
    throw error;
  }
}

async function main(argv) {
  const names = new Map([
    ["--package", "package"],
    ["--package-sha256", "packageSha256"],
    ["--root", "root"],
    ["--retained-package", "retainedPackage"],
    ["--retained-package-sha256", "retainedPackageSha256"],
    ["--retained-root", "retainedRoot"],
    ["--cert", "cert"],
    ["--key", "key"],
    ["--profile", "profile"],
    ["--port", "port"],
  ]);
  const options = {};
  for (let index = 0; index < argv.length; index += 2) {
    const name = names.get(argv[index]);
    if (!name || !argv[index + 1] || Object.hasOwn(options, name))
      fail(
        "Expected unique --package --package-sha256 --root --cert --key --profile [--port] [--retained-package --retained-package-sha256 --retained-root]",
      );
    options[name] = name === "port" ? Number(argv[index + 1]) : argv[index + 1];
  }
  const service = await startCodeAssetService(options);
  process.stdout.write(
    JSON.stringify({ origin: service.origin, receipt: service.receiptPath }) + "\n",
  );
  for (const signal of ["SIGINT", "SIGTERM"])
    process.once(signal, () => {
      service.close().then(
        () => {
          process.exitCode = 0;
        },
        () => {
          process.exitCode = 1;
        },
      );
    });
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main(process.argv.slice(2)).catch((error) => {
    process.stderr.write(`Code asset service refused: ${error.message}\n`);
    process.exitCode = 1;
  });
}
