#!/usr/bin/env node
import { spawn } from "node:child_process";
import { readFile } from "node:fs/promises";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { inspectExpoWebArtifact } from "./expo-web-artifact.mjs";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const flags = process.argv.slice(2);
if (flags.some((flag) => flag !== "--dry-run"))
  throw new Error("Usage: node scripts/deploy-expo-web.mjs [--dry-run]");
const providers = JSON.parse(
  await readFile(join(root, "apps/zoen/config/identity-providers.json")),
);
for (const url of [providers.relayUrl, providers.webOrigin]) {
  const parsed = new URL(url);
  if (parsed.protocol !== "https:" || parsed.username || parsed.password)
    throw new Error("Cloudflare releases require the configured public HTTPS endpoints.");
}
const env = {
  ...process.env,
  EXPO_NO_DOTENV: "1",
  EXPO_PUBLIC_ZOEN_RELAY_URL: providers.relayUrl,
  EXPO_PUBLIC_GOOGLE_IOS_CLIENT_ID: providers.googleIosClientId,
  EXPO_PUBLIC_GOOGLE_WEB_CLIENT_ID: providers.googleWebClientId,
};
if (
  env.EXPO_PUBLIC_APPLE_REDIRECT_URI &&
  new URL(env.EXPO_PUBLIC_APPLE_REDIRECT_URI).origin !== providers.webOrigin
)
  throw new Error("Apple redirect must use the configured Web origin.");

function run(command, args) {
  return new Promise((resolve, reject) => {
    const child = spawn(command, args, { cwd: root, env, stdio: "inherit" });
    child.once("error", reject);
    child.once("close", (code, signal) =>
      code === 0 ? resolve() : reject(new Error(`${command} failed (${signal ?? code}).`)),
    );
  });
}

// Always build a release with explicit public settings; never upload a QA export
// that may contain a loopback relay or stale signing worker integrity values.
await run("npm", ["run", "export:web", "--prefix", "apps/zoen"]);
const artifact = await inspectExpoWebArtifact(join(root, "apps/zoen/dist"));
for (const file of artifact.manifest)
  if (file.bytes > 25 * 1024 * 1024)
    throw new Error(`Cloudflare asset exceeds 25 MiB: ${file.path}`);
await run(process.execPath, [
  join(root, "infra/cloudflare/node_modules/wrangler/bin/wrangler.js"),
  "deploy",
  "--config",
  "infra/cloudflare/web/wrangler.jsonc",
  ...(flags.includes("--dry-run") ? ["--dry-run"] : []),
]);
