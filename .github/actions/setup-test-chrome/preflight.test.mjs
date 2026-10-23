import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { chmod, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { promisify } from "node:util";
import { commandVersionMatches, protocolVersionMatches, preflightChrome } from "./preflight.mjs";

const version = "156.0.8078.47";
const canary = "SYNTHETIC_BROWSER_OUTPUT_MUST_NOT_BE_REPORTED";

test("version admission requires the exact CfT command and protocol versions", () => {
  assert.equal(commandVersionMatches(`Google Chrome for Testing ${version}\n`, version), true);
  for (const value of [
    `Google Chrome ${version}`,
    `Google Chrome for Testing ${version}.1`,
    `Google Chrome for Testing ${version}\n${canary}`,
    `Google Chrome for Testing 155.0.8078.47`,
  ])
    assert.equal(commandVersionMatches(value, version), false);
  assert.equal(protocolVersionMatches(`Chrome/${version}`, version), true);
  assert.equal(protocolVersionMatches(`HeadlessChrome/${version}`, version), true);
  for (const product of [undefined, {}, `Chrome/${version}.1`, `Chrome/155.0.8078.47`, canary]) {
    assert.equal(protocolVersionMatches(product, version), false);
  }
});

async function fixture(run) {
  const folder = await mkdtemp(join(tmpdir(), "zoen-cft-selection-test-"));
  const path = join(folder, "synthetic-browser");
  try {
    await run({ folder, path });
  } finally {
    await rm(folder, { recursive: true, force: true });
  }
}

async function executable(path, body) {
  await writeFile(path, `#!${process.execPath}\n${body}\n`);
  await chmod(path, 0o755);
}

test("a wrong command version fails before any browser startup", async () => {
  await fixture(async ({ folder, path }) => {
    const marker = join(folder, "spawned");
    await executable(
      path,
      `require('node:fs').writeFileSync(${JSON.stringify(marker)}, process.argv.slice(2).join(' ')); console.log(${JSON.stringify(canary)});`,
    );
    await assert.rejects(preflightChrome(path, version), (error) => {
      assert.equal(error.phase, "version");
      assert.equal(error.message.includes(canary), false);
      return true;
    });
    assert.equal(await readFile(marker, "utf8"), "--version");
  });
});

test("a version string without real DevTools never admits a ready preflight or leaks raw output", async () => {
  await fixture(async ({ folder, path }) => {
    const marker = join(folder, "pid");
    await executable(
      path,
      `if(process.argv.includes('--version')) console.log('Google Chrome for Testing ${version}'); else {require('node:fs').writeFileSync(${JSON.stringify(marker)},String(process.pid)); console.log(${JSON.stringify(canary)}); console.error(${JSON.stringify(canary)}); process.exitCode=23;}`,
    );
    let output;
    try {
      await promisify(execFile)(
        process.execPath,
        [fileURLToPath(new URL("./preflight.mjs", import.meta.url))],
        {
          env: {
            PATH: process.env.PATH,
            HOME: folder,
            CHROME_BIN: path,
            ZOEN_PINNED_CHROME_VERSION: version,
          },
          timeout: 15_000,
          maxBuffer: 8192,
        },
      );
      assert.fail("a process exiting before DevTools must fail");
    } catch (error) {
      assert.equal(error.code, 1);
      output = String(error.stdout) + String(error.stderr);
    }
    assert.equal(output.includes(canary), false);
    const result = JSON.parse(output.trim());
    assert.equal(result.status, "failed");
    assert.equal(result.phase, "startup");
    assert.equal(Object.hasOwn(result, "protocolVersionMatched"), false);
    const pid = Number(await readFile(marker, "utf8"));
    assert.throws(() => process.kill(pid, 0), { code: "ESRCH" });
  });
});

test("the preflight command rejects a missing pinned path before fallback", async () => {
  try {
    await promisify(execFile)(
      process.execPath,
      [fileURLToPath(new URL("./preflight.mjs", import.meta.url))],
      {
        env: { PATH: process.env.PATH, CHROME_BIN: "", ZOEN_PINNED_CHROME_VERSION: version },
        timeout: 3000,
        maxBuffer: 8192,
      },
    );
    assert.fail("missing explicit artifact must fail");
  } catch (error) {
    assert.equal(error.code, 1);
    assert.equal(JSON.parse(error.stderr).phase, "configuration");
  }
});
