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
import { acceptBuiltinMiniappJourney } from "./expo-web-miniapp-journey.mjs";
import { acceptMediaJourney } from "./expo-web-media-journey.mjs";
import { acceptDrawingJourney } from "./expo-web-drawing-journey.mjs";
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
  journey: "expo-linked-browser-release-v4",
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
  const started = new Date().toISOString();
  const process = start(cliPath, ["--home", home, "--timeout", "30000", ...args], {
    env: { ...globalThis.process.env, ZOEN_NET_DEBUG: "1" },
  });
  const result = await Promise.race([process.done, delay(150_000, null, { ref: false })]);
  if (!result) {
    await stop(process);
    throw new Error(`CLI ${args[0]} timed out`);
  }
  logs.push(
    `CLI ${args[0]} [${started} → ${new Date().toISOString()}]: ${scrub(result.stdout + result.stderr)}`,
  );
  assert.equal(result.code, 0, `CLI ${args[0]} failed: ${scrub(result.stderr)}`);
  return result.stdout;
}
async function watcher(home) {
  const process = start(cliPath, ["--home", home, "--timeout", "30000", "watch", "--for", "600"], {
    env: { ...globalThis.process.env, ZOEN_NET_DEBUG: "1" },
  });
  process.watcherStarted = new Date().toISOString();
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
async function expectMessage(author, text, timeout = 90_000, messageId) {
  // A hidden conversation preview can repeat the text. Require its exact author
  // and the real interactive message bubble, then the visible body inside it.
  // The accessible name also carries localized quote/time/delivery metadata.
  // Check the complete actual body separately so neither a matching preview
  // nor quoted text or a longer body can satisfy this assertion.
  const escape = (value) => value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const name = new RegExp(`^${escape(`${author}: `)}`);
  const bubble = page
    .getByTestId(messageId ? `message-bubble-${messageId}` : /^message-bubble-/)
    .and(page.getByRole("button", { name }));
  const body = bubble
    .getByTestId(/^message-body-/)
    .filter({ hasText: new RegExp(`^${escape(text)}$`) });
  await body.waitFor({ timeout });
  assert.equal(await body.innerText(), text, "the visible message body must match in full");
  const testId = await bubble.getAttribute("data-testid");
  assert.match(testId, /^message-bubble-.+$/, "the original message must have its real ID");
  return testId.slice("message-bubble-".length);
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
  let peer = await watcher(bruno.home);
  const dm = await cli(ana.home, "dm", `@${bruno.handle}`, seed);
  const space = dm
    .trim()
    .split(/\s+/)
    .findLast((value) => /^sp_[a-z0-9]+$/.test(value));
  assert.ok(space, "CLI did not return a canonical DM id");
  // Establish the peer's durable baseline before linking or erasure. A sender's
  // local timeline alone does not prove that the first MLS message reached its peer.
  // Do not resend a missing seed: that would conceal a bootstrap/delivery failure.
  await until(
    () => {
      if (peer.child.exitCode !== null)
        throw new Error(`Peer exited before receiving the seed: ${scrub(peer.output())}`);
      return peer.output().includes(`${ana.name}: ${seed}`);
    },
    "canonical peer did not receive the original DM seed before linking",
    30_000,
  );
  await stop(peer);
  for (const person of [ana, bruno]) {
    const sync = await cli(person.home, "sync");
    assert.match(
      sync,
      /connection=online synced=true pending=0(?:\s|$)/,
      `${person.name} must confirm the canonical seed before linking`,
    );
    const history = await cli(person.home, "read", space);
    assert.ok(
      history.includes(seed),
      `${person.name} cold read must retain the original seed before linking`,
    );
    assert.ok(
      !history.includes(`${seed} (sending)`) && !history.includes(`${seed} (failed)`),
      `${person.name} seed must be confirmed, not pending or failed`,
    );
  }
  peer = await watcher(bruno.home);
  step("synthetic-encrypted-history-created", { peerSeedConfirmedBeforeLink: true });

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
  // Successful linking opens Home. Reach the actual conversation list through
  // its visible navigation link before checking the exact recovered preview.
  await page.locator('a[href="/chats"]').click();
  await page.waitForURL((url) => url.pathname === "/chats");
  const row = page.getByRole("button", { name: new RegExp(seed) });
  await row.waitFor({ timeout: 120_000 });
  await row.click();
  await page.waitForURL(`**/chat/${space}`);
  const seedMessageId = await expectMessage(ana.name, seed, 120_000);
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
  const replyMessageId = await expectMessage(bruno.name, reply);
  assert.ok(sentFrames > 0, "journey must observe actual relay frames");
  assert.equal(plaintextOnWire, false, "private text must not appear in outbound relay frames");
  await screenshot("03-cross-core-message-roundtrip.png");
  step("expo-to-native-core-message-roundtrip", { outboundPlaintextSeenOnWire: false });

  const reload = await page.reload({ waitUntil: "domcontentloaded" });
  assert.equal(reload?.status(), 200, "direct dynamic chat reload must survive real Pages hosting");
  assert.equal(new URL(page.url()).pathname, `/chat/${space}`);
  await page.getByTestId("message-input").waitFor();
  await expectMessage(ana.name, seed, 90_000, seedMessageId);
  const outboundMessageId = await expectMessage(ana.name, outbound);
  await expectMessage(bruno.name, reply, 90_000, replyMessageId);
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
  await acceptBuiltinMiniappJourney({
    page,
    ui,
    origin,
    space,
    tag,
    peerHome: bruno.home,
    cli,
    screenshot,
    step,
  });
  await acceptMediaJourney({
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
  await acceptDrawingJourney({
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
  // Later miniapp activities correctly reopen this virtualized conversation at
  // its newest rows. Reveal each original through the real search UI rather
  // than requiring an old message to already be inside the viewport.
  await page.getByRole("button", { name: ui.searchThisChat, exact: true }).click();
  const retainedMessages = [
    { author: ana.name, text: seed, id: seedMessageId },
    { author: ana.name, text: outbound, id: outboundMessageId },
    { author: bruno.name, text: reply, id: replyMessageId },
  ];
  for (const message of retainedMessages) {
    await page.getByTestId("message-input").fill(message.text);
    await page.getByTestId("message-input").press("Enter");
    await expectMessage(message.author, message.text, 90_000, message.id);
  }
  await page.getByTestId("close-conversation-search").click();
  step("cancel-local-erasure-retains-account-and-encrypted-history", {
    originalMessageIds: retainedMessages.map((message) => message.id),
  });

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
  for (const process of processes.values())
    if (process.watcherStarted)
      logs.push(`Peer watcher [${process.watcherStarted}]: ${scrub(process.output())}`);
  await mkdir(evidencePath, { recursive: true });
  await writeFile(join(evidencePath, "acceptance.json"), JSON.stringify(evidence, null, 2));
  await writeFile(join(evidencePath, "sanitized.log"), logs.map(scrub).join("\n"));
  await rm(privateRoot, { recursive: true, force: true });
}
