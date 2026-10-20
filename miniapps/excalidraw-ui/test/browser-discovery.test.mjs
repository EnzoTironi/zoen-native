import assert from "node:assert/strict";
import { chmod, mkdir, mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { findBrowserExecutable } from "./browser-driver.mjs";

async function executable(folder, name, mode = 0o755) {
  const path = join(folder, name);
  await writeFile(path, "synthetic discovery fixture; never executed\n");
  await chmod(path, mode);
  return path;
}
async function withFolder(run) {
  const folder = await mkdtemp(join(tmpdir(), "zoen-browser-discovery-"));
  try {
    await run(folder);
  } finally {
    await rm(folder, { recursive: true, force: true });
  }
}
const available =
  (...paths) =>
  async (path) =>
    paths.includes(path);

test("explicit executable CHROME_BIN with spaces wins over automatic discovery", async () => {
  await withFolder(async (folder) => {
    const path = await executable(folder, "Synthetic Chrome");
    assert.equal(await findBrowserExecutable({ env: { CHROME_BIN: path, PATH: folder } }), path);
  });
});
test("an explicit browser command resolves through the supplied PATH", async () => {
  await withFolder(async (folder) => {
    const path = await executable(folder, "custom-chrome");
    assert.equal(
      await findBrowserExecutable({ env: { CHROME_BIN: "custom-chrome", PATH: folder } }),
      path,
    );
  });
});
test("invalid explicit browser files fail rather than quietly falling back or skipping", async () => {
  await withFolder(async (folder) => {
    await executable(folder, "google-chrome");
    const directory = join(folder, "directory");
    await mkdir(directory);
    await assert.rejects(
      findBrowserExecutable({ env: { CHROME_BIN: directory, PATH: folder } }),
      /CHROME_BIN is not an executable browser file/,
    );
    await assert.rejects(
      findBrowserExecutable({ env: { CHROME_BIN: join(folder, "missing"), PATH: folder } }),
      /CHROME_BIN is not an executable browser file/,
    );
    if (process.platform !== "win32") {
      const denied = await executable(folder, "not-executable", 0o644);
      await assert.rejects(
        findBrowserExecutable({ env: { CHROME_BIN: denied, PATH: folder } }),
        /CHROME_BIN is not an executable browser file/,
      );
    }
  });
});
test("Linux checks installed browser locations before PATH", async () => {
  assert.equal(
    await findBrowserExecutable({
      platform: "linux",
      env: { PATH: "/synthetic/bin" },
      canExecute: available("/usr/bin/chromium", "/synthetic/bin/google-chrome"),
    }),
    "/usr/bin/chromium",
  );
});
test("PATH fallback respects executable name priority and directory order", async () => {
  assert.equal(
    await findBrowserExecutable({
      platform: "linux",
      env: { PATH: "/first:/second" },
      canExecute: available("/first/chromium", "/second/google-chrome"),
    }),
    "/second/google-chrome",
  );
  assert.equal(
    await findBrowserExecutable({
      platform: "linux",
      env: { PATH: "/first:/second" },
      canExecute: available("/first/google-chrome", "/second/google-chrome"),
    }),
    "/first/google-chrome",
  );
});
test("macOS supports a user-local application without relying on system Chrome", async () => {
  const path = "/synthetic/user/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";
  assert.equal(
    await findBrowserExecutable({
      platform: "darwin",
      env: { HOME: "/synthetic/user" },
      canExecute: available(path),
    }),
    path,
  );
});
test("Windows supports installed Chrome and semicolon-separated PATH", async () => {
  const installed = "C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe";
  assert.equal(
    await findBrowserExecutable({
      platform: "win32",
      env: { PROGRAMFILES: "C:\\Program Files" },
      canExecute: available(installed),
    }),
    installed,
  );
  const fromPath = "D:\\tools\\chromium.exe";
  assert.equal(
    await findBrowserExecutable({
      platform: "win32",
      env: { PATH: "C:\\missing;D:\\tools" },
      canExecute: available(fromPath),
    }),
    fromPath,
  );
});
test("missing browsers fail in CI but permit an explicit local skip", async () => {
  for (const env of [{ CI: "true" }, { CI: "1" }, { GITHUB_ACTIONS: "true", CI: "false" }]) {
    await assert.rejects(
      findBrowserExecutable({ platform: "linux", env, canExecute: available() }),
      /required for CI renderer tests/,
    );
  }
  for (const env of [{}, { CI: "false" }, { CI: "0" }]) {
    assert.equal(
      await findBrowserExecutable({ platform: "linux", env, canExecute: available() }),
      null,
    );
  }
});
