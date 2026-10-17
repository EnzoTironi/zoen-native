#!/usr/bin/env node
// Real Expo export + Cloudflare Pages + Rust CLI peers. No IDB injection or auth mocks.
import assert from "node:assert/strict";
import { createHash, randomBytes } from "node:crypto";
import { spawn, execFileSync } from "node:child_process";
import { mkdir, mkdtemp, rm, writeFile } from "node:fs/promises";
import { createServer } from "node:net";
import { createRequire } from "node:module";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { setTimeout as delay } from "node:timers/promises";
import { inspectExpoWebArtifact } from "./expo-web-artifact.mjs";
import { verifyExpoWebHosting } from "./expo-web-hosting.mjs";
import { acceptMarkdownJourney } from "./expo-web-markdown-journey.mjs";
import { expoWebExpectations, pairingScreenshotMasks } from "./expo-web-expectations.mjs";

const locale = process.env.ZOEN_EXPO_TEST_LOCALE;
const ui = expoWebExpectations(locale);
const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const dist = resolve(process.env.ZOEN_EXPO_DIST ?? join(root, "apps/zoen/dist"));
const tools = resolve(process.env.ZOEN_E2E_TOOLS ?? join(root, "scripts/expo-web-tests"));
const require = createRequire(join(tools, "package.json"));
const { chromium } = require("playwright-core");
const cliPath = resolve(process.env.ZOEN_CLI ?? join(root, "target/debug/zoen"));
const relay = new URL(process.env.RELAY ?? "http://127.0.0.1:8790");
assert.ok(
  ["127.0.0.1", "localhost", "[::1]"].includes(relay.hostname) &&
    relay.protocol === "http:" &&
    relay.pathname === "/" &&
    !relay.username &&
    !relay.password &&
    !relay.search &&
    !relay.hash,
  "this synthetic-account journey requires a loopback relay",
);
const evidenceRoot = resolve(
  process.env.ZOEN_EXPO_EVIDENCE ?? join(root, ".dev/expo-web-evidence"),
);
const evidencePath = join(evidenceRoot, locale);
const privateRoot = await mkdtemp(join(tmpdir(), "zoen-expo-acceptance-"));
const children = new Set();
const processes = new Map();
const logs = [];
const errors = [];
const evidence = {
  journey: "expo-linked-browser-release-v3",
  locale,
  commit: execFileSync("git", ["rev-parse", "HEAD"], { cwd: root, encoding: "utf8" }).trim(),
  steps: [],
  passed: false,
};
const tag = createHash("sha256")
  .update(
    `${process.env.GITHUB_RUN_ID ?? randomBytes(12).toString("hex")}:${process.env.GITHUB_RUN_ATTEMPT ?? 1}:${locale}`,
  )
  .digest("hex")
  .slice(0, 10);
const seed = `Expo acceptance history ${tag}`;
const outbound = `Expo acceptance browser message ${tag}`;
const reply = `Expo acceptance peer reply ${tag}`;
let browser, page;
let plaintextOnWire = false;
let sentFrames = 0;

// Logs and screenshots must not retain pair secrets, JWTs or provider credentials.
function scrub(input) {
  return String(input)
    .replace(/zoen-link:1:[a-f0-9]+/gi, "[pairing-code-redacted]")
    .replace(/eyJ[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+/g, "[jwt-redacted]")
    .replace(/\b[a-f0-9]{64,}\b/gi, "[key-or-id-redacted]");
}
function step(name, details = {}) {
  evidence.steps.push({ name, ...details });
  console.log(`PASS [${locale}] ${name}`);
}
function start(command, args, options = {}) {
  const child = spawn(command, args, {
    cwd: root,
    ...options,
    detached: globalThis.process.platform !== "win32",
    stdio: ["ignore", "pipe", "pipe"],
  });
  children.add(child);
  let stdout = "",
    stderr = "";
  child.stdout.on("data", (chunk) => {
    stdout = `${stdout}${chunk}`.slice(-2_000_000);
  });
  child.stderr.on("data", (chunk) => {
    stderr = `${stderr}${chunk}`.slice(-2_000_000);
  });
  const done = new Promise((resolveDone) => {
    child.once("error", (error) => {
      children.delete(child);
      stderr += error.message;
      resolveDone({ code: -1, signal: null, stdout, stderr });
    });
    child.once("exit", (code, signal) => {
      children.delete(child);
      resolveDone({ code, signal, stdout, stderr });
    });
  });
  const process = { child, done, output: () => stdout + stderr };
  processes.set(child, process);
  return process;
}
async function stop(process) {
  if (!process || process.child.exitCode !== null || process.child.signalCode !== null) return;
  const signal = (value) => {
    try {
      if (globalThis.process.platform !== "win32" && process.child.pid)
        globalThis.process.kill(-process.child.pid, value);
      else process.child.kill(value);
    } catch (error) {
      if (error.code !== "ESRCH") throw error;
    }
  };
  signal("SIGINT");
  if (!(await Promise.race([process.done.then(() => true), delay(3000, false, { ref: false })]))) {
    signal("SIGKILL");
    await process.done;
  }
}
async function until(predicate, message, timeout = 60_000) {
  const end = Date.now() + timeout;
  while (Date.now() < end) {
    if (await predicate()) return;
    await delay(100);
  }
  throw new Error(message);
}
async function cli(home, ...args) {
  const process = start(cliPath, ["--home", home, "--timeout", "30000", ...args]);
  const result = await Promise.race([process.done, delay(150_000, null, { ref: false })]);
  if (!result) {
    await stop(process);
    throw new Error(`CLI ${args[0]} timed out`);
  }
  logs.push(`CLI ${args[0]}: ${scrub(result.stdout + result.stderr)}`);
  assert.equal(result.code, 0, `CLI ${args[0]} failed: ${scrub(result.stderr)}`);
  return result.stdout;
}
async function watcher(home) {
  const process = start(cliPath, ["--home", home, "--timeout", "30000", "watch", "--for", "600"]);
  await until(() => {
    if (process.child.exitCode !== null) throw new Error(`Peer exited: ${scrub(process.output())}`);
    return /watching as @/.test(process.output());
  }, "peer did not finish canonical sync");
  return process;
}
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
async function expectMessage(author, text, timeout = 90_000) {
  // A hidden conversation preview can repeat the text. Require its exact author
  // and the real interactive message bubble, then the visible body inside it.
  const bubble = page.getByRole("button", { name: `${author}: ${text}`, exact: true });
  await bubble.getByText(text, { exact: true }).waitFor({ timeout });
}
async function screenshot(name) {
  await page.screenshot({
    path: join(evidencePath, name),
    fullPage: true,
    mask: pairingScreenshotMasks(page),
  });
}
try {
  await mkdir(evidencePath, { recursive: true });
  evidence.artifact = await inspectExpoWebArtifact(dist);
  step("release-asset-integrity");
  const health = await fetch(new URL("/healthz", relay), { signal: AbortSignal.timeout(5000) });
  assert.ok(health.ok, "loopback acceptance relay is not healthy");
  await health.arrayBuffer();
  const [port, inspector] = await Promise.all([freePort(), freePort()]);
  const pages = start(
    process.execPath,
    [
      join(tools, "node_modules/wrangler/bin/wrangler.js"),
      "pages",
      "dev",
      dist,
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
      join(privateRoot, "pages-state"),
    ],
    {
      cwd: privateRoot,
      env: {
        ...process.env,
        WRANGLER_SEND_METRICS: "false",
        WRANGLER_LOG_PATH: join(privateRoot, "wrangler.log"),
      },
    },
  );
  const origin = `http://127.0.0.1:${port}`;
  await until(async () => {
    if (pages.child.exitCode !== null) throw new Error(`Pages failed: ${scrub(pages.output())}`);
    try {
      const response = await fetch(origin, { signal: AbortSignal.timeout(500) });
      await response.body?.cancel();
      return response.ok;
    } catch {
      return false;
    }
  }, "Cloudflare Pages did not start");
  const paths = await verifyExpoWebHosting(origin, dist);
  step("cloudflare-auth-isolation-and-dynamic-route-policy", { paths });

  const ana = { home: join(privateRoot, "ana"), handle: `qa_expoa_${tag}`, name: "Expo QA Ana" };
  const bruno = {
    home: join(privateRoot, "bruno"),
    handle: `qa_expob_${tag}`,
    name: "Expo QA Bruno",
  };
  for (const person of [ana, bruno])
    await cli(
      person.home,
      "init",
      "--name",
      person.name,
      "--handle",
      person.handle,
      "--relay",
      relay.origin,
    );
  const peer = await watcher(bruno.home);
  const dm = await cli(ana.home, "dm", `@${bruno.handle}`, seed);
  const space = dm
    .trim()
    .split(/\s+/)
    .findLast((value) => /^sp_[a-z0-9]+$/.test(value));
  assert.ok(space, "CLI did not return a canonical DM id");
  step("synthetic-encrypted-history-created");

  browser = await chromium.launch({
    executablePath: process.env.CHROME ?? "/usr/bin/google-chrome",
    args: ["--no-sandbox"],
  });
  evidence.browser = browser.version();
  const context = await browser.newContext({
    viewport: { width: 1280, height: 820 },
    locale,
  });
  page = await context.newPage();
  page.setDefaultTimeout(30_000);
  page.on("pageerror", (error) => errors.push(scrub(error.message)));
  page.on("websocket", (socket) =>
    socket.on("framesent", (frame) => {
      sentFrames++;
      if (Buffer.from(frame.payload).includes(Buffer.from(outbound))) plaintextOnWire = true;
    }),
  );
  await page.goto(origin, { waitUntil: "domcontentloaded" });
  await page.waitForURL("**/sign-in");
  evidence.browserLocale = await page.evaluate(() => navigator.language);
  assert.equal(
    evidence.browserLocale,
    locale,
    "browser locale must match this explicit acceptance run",
  );
  assert.equal(
    await page.evaluate(() => crossOriginIsolated),
    false,
    "provider entry must allow popups",
  );
  assert.equal(
    await page.getByRole("button", { name: ui.developmentOptions, exact: true }).count(),
    0,
    "release build must not expose development controls",
  );
  await screenshot("01-release-onboarding.png");
  await page.getByRole("button", { name: ui.connectOtherDevice, exact: true }).click();
  await page.getByRole("button", { name: ui.showPairingCode, exact: true }).click();
  const code = (await page.getByLabel(ui.pairingCode, { exact: true }).innerText()).trim();
  assert.match(code, /^zoen-link:1:[a-f0-9]{192}$/);
  const digits = (
    await page.locator(`[aria-label^=${JSON.stringify(ui.verificationPrefix)}]`).innerText()
  ).replace(/\s/g, "");
  const [linked] = await Promise.all([
    cli(ana.home, "link", code),
    page.waitForURL((url) => url.pathname === "/", { timeout: 120_000 }),
  ]);
  assert.equal(
    linked.match(/check:\s*(\d{6})/)?.[1],
    digits,
    "both canonical pairing sides must show the same digits",
  );
  assert.equal(
    await page.evaluate(() => crossOriginIsolated && typeof SharedArrayBuffer !== "undefined"),
    true,
    "MLS application must be isolated after linking",
  );
  const row = page.getByRole("button", { name: new RegExp(seed) });
  await row.waitFor({ timeout: 120_000 });
  await row.click();
  await page.waitForURL(`**/chat/${space}`);
  await expectMessage(ana.name, seed, 120_000);
  await screenshot("02-recovered-encrypted-history.png");
  step("normal-device-link-and-encrypted-history-recovery", { checkDigitsMatch: true });

  await page.getByTestId("message-input").fill(outbound);
  await page.getByTestId("send-message").click();
  await until(
    () => peer.output().includes(outbound),
    "real CLI peer did not decrypt the Expo message",
    90_000,
  );
  await stop(peer);
  const read = await cli(bruno.home, "read", space);
  assert.ok(
    read.includes(outbound),
    "peer canonical persisted timeline must contain the browser message",
  );
  await cli(bruno.home, "send", space, reply);
  await expectMessage(bruno.name, reply);
  assert.ok(sentFrames > 0, "journey must observe actual relay frames");
  assert.equal(plaintextOnWire, false, "private text must not appear in outbound relay frames");
  await screenshot("03-cross-core-message-roundtrip.png");
  step("expo-to-native-core-message-roundtrip", { outboundPlaintextSeenOnWire: false });

  const reload = await page.reload({ waitUntil: "domcontentloaded" });
  assert.equal(reload?.status(), 200, "direct dynamic chat reload must survive real Pages hosting");
  assert.equal(new URL(page.url()).pathname, `/chat/${space}`);
  await page.getByTestId("message-input").waitFor();
  await expectMessage(ana.name, seed);
  await expectMessage(ana.name, outbound);
  await expectMessage(bruno.name, reply);
  assert.equal(
    await page.getByRole("button", { name: ui.connectOtherDevice, exact: true }).count(),
    0,
    "cold reload must retain the encrypted browser session",
  );
  assert.equal(
    await page.getByRole("alert").count(),
    0,
    "a valid cold-reloaded synthetic chat must not show a session or integrity error",
  );
  await screenshot("04-cold-chat-reload.png");
  step("cold-dynamic-chat-reload-retains-keys-and-history");
  await acceptMarkdownJourney({
    page,
    ui,
    origin,
    space,
    tag,
    privateRoot,
    peerHome: bruno.home,
    cli,
    screenshot,
    step,
  });
  await page.goto(`${origin}/you`, { waitUntil: "domcontentloaded" });
  await page.getByText(`@${ana.handle}`, { exact: true }).waitFor();
  await page.getByTestId("erase-device").click();
  const confirmation = page.getByRole("dialog", { name: ui.eraseDialog, exact: true });
  await confirmation.getByRole("button", { name: ui.cancel, exact: true }).click();
  await confirmation.waitFor({ state: "hidden" });
  await page.goto(`${origin}/chat/${space}`, { waitUntil: "domcontentloaded" });
  await expectMessage(ana.name, seed);
  await expectMessage(ana.name, outbound);
  await expectMessage(bruno.name, reply);
  step("cancel-local-erasure-retains-account-and-encrypted-history");

  await page.goto(`${origin}/you`, { waitUntil: "domcontentloaded" });
  await page.getByText(`@${ana.handle}`, { exact: true }).waitFor();
  await page.getByTestId("erase-device").click();
  await confirmation.getByRole("button", { name: ui.confirmErase, exact: true }).click();
  await page.waitForURL("**/sign-in", { timeout: 90_000 });
  await page.getByRole("button", { name: ui.connectOtherDevice, exact: true }).waitFor();
  assert.equal(await page.evaluate(() => crossOriginIsolated), false);
  await page.reload({ waitUntil: "domcontentloaded" });
  await page.getByRole("button", { name: ui.connectOtherDevice, exact: true }).waitFor();
  assert.equal(new URL(page.url()).pathname, "/sign-in");
  assert.equal(await page.getByTestId("erase-device").count(), 0);
  assert.equal(await page.getByTestId("message-input").count(), 0);
  await screenshot("05-erased-browser-cold-onboarding.png");
  step("confirmed-local-erasure-cold-reload-keeps-onboarding");

  const preserved = await cli(bruno.home, "read", space);
  for (const text of [seed, outbound, reply])
    assert.ok(preserved.includes(text), "local browser erase must preserve canonical peer history");
  step("browser-erasure-preserves-canonical-peer-history");
  assert.deepEqual(errors, [], "uncaught browser errors");
  evidence.passed = true;
} catch (error) {
  evidence.failure = scrub(error?.stack ?? error);
  if (page) await screenshot("failure-redacted.png").catch(() => {});
  throw new Error(evidence.failure);
} finally {
  evidence.browserErrors = [...errors];
  await browser?.close().catch(() => {});
  await Promise.allSettled([...children].map((child) => stop(processes.get(child))));
  await mkdir(evidencePath, { recursive: true });
  await writeFile(join(evidencePath, "acceptance.json"), JSON.stringify(evidence, null, 2));
  await writeFile(join(evidencePath, "sanitized.log"), logs.map(scrub).join("\n"));
  await rm(privateRoot, { recursive: true, force: true });
}
