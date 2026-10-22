import assert from "node:assert/strict";
import { mkdtemp, mkdir, rm, symlink, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import {
  annotateOwnedBrowserFailure,
  boundedDiagnosticFile,
  inspectOwnedBrowserStartup,
} from "../../../tools/miniapp-build/browser-startup.ts";

const child = () => ({ pid: 42, exitCode: null, signalCode: null });
const active = "12345\n/devtools/browser/a24582bd-0377-4fee-bb76-73d465c927c3\n";

function fixture(overrides = {}) {
  const reads = [];
  return {
    reads,
    system: {
      platform: "linux",
      async read(path) {
        reads.push(path);
        if (path === "/owned/profile/DevToolsActivePort") return active;
        if (path === "/proc/42/stat")
          return "42 (chrome (with spaces)) S 1 2 3 4 5 6 7 8 9 10 123 456 0 0";
        if (path === "/proc/42/wchan") return "futex_wait_queue\n";
        throw Error("Unexpected diagnostic path");
      },
      async link(path) {
        reads.push(path);
        assert.equal(path, "/proc/42/exe");
        return "/opt/google/chrome/chrome";
      },
      ...overrides,
    },
  };
}

test("startup diagnostics retain only owned live PID fields and endpoint presence, not raw stat or URL", async () => {
  const { system, reads } = fixture();
  const result = await inspectOwnedBrowserStartup(child(), "/owned/profile", system);
  assert.deepEqual(result, {
    activePort: "valid",
    process: "observed",
    executable: "chrome",
    state: "S",
    waitChannel: "futex_wait_queue",
    userCpuTicks: 123,
    systemCpuTicks: 456,
  });
  assert.deepEqual(
    new Set(reads),
    new Set([
      "/owned/profile/DevToolsActivePort",
      "/proc/42/stat",
      "/proc/42/wchan",
      "/proc/42/exe",
    ]),
  );
  assert.doesNotMatch(JSON.stringify(result), /12345|devtools|with spaces|\/opt|\/proc/);
});

test("exited or unspawned children never read a reused or unrelated process", async () => {
  for (const current of [
    { pid: 42, exitCode: 7, signalCode: null },
    { pid: 42, exitCode: null, signalCode: "SIGKILL" },
    { exitCode: null, signalCode: null },
  ]) {
    const { system, reads } = fixture();
    assert.deepEqual(await inspectOwnedBrowserStartup(current, "/owned/profile", system), {
      activePort: "valid",
      process: "exited",
    });
    assert.deepEqual(reads, ["/owned/profile/DevToolsActivePort"]);
  }
});

test("non-Linux hosts do not attempt Linux process inspection", async () => {
  const { system, reads } = fixture({ platform: "darwin" });
  assert.deepEqual(await inspectOwnedBrowserStartup(child(), "/owned/profile", system), {
    activePort: "valid",
    process: "unsupported",
  });
  assert.deepEqual(reads, ["/owned/profile/DevToolsActivePort"]);
});

test("malformed endpoint and untrusted process fields cannot add raw diagnostic data", async () => {
  const canary = "UNTRUSTED_SECRET_CANARY";
  const { system } = fixture({
    async read(path) {
      if (path.endsWith("DevToolsActivePort")) return `65536\n${canary}`;
      if (path.endsWith("/wchan")) return canary + "/not-a-kernel-symbol";
      return "42 (private comm) ? 1 2 3 4 5 6 7 8 9 10 NaN -1";
    },
    async link() {
      return `/private/${canary}`;
    },
  });
  const result = await inspectOwnedBrowserStartup(child(), "/owned/profile", system);
  assert.deepEqual(result, {
    activePort: "invalid",
    process: "observed",
    executable: "other",
    state: null,
    waitChannel: null,
    userCpuTicks: null,
    systemCpuTicks: null,
  });
  assert.doesNotMatch(JSON.stringify(result), /CANARY|private|NaN/);
});

test("process disappearance or malformed PID stat preserves the profile observation without repair", async () => {
  for (const value of [null, "999 (chrome) S"]) {
    const base = fixture();
    const system = {
      ...base.system,
      async read(path, limit) {
        if (path.endsWith("/stat")) {
          if (value === null) throw Error("gone");
          return value;
        }
        return base.system.read(path, limit);
      },
    };
    assert.deepEqual(await inspectOwnedBrowserStartup(child(), "/owned/profile", system), {
      activePort: "valid",
      process: "unavailable",
    });
  }
});

test("a stuck diagnostic does not postpone owned browser cleanup beyond its separate 200 ms cap", async (t) => {
  t.mock.timers.enable({ apis: ["setTimeout"] });
  const { system } = fixture({ read: () => new Promise(() => {}) });
  const observed = inspectOwnedBrowserStartup(child(), "/owned/profile", system);
  t.mock.timers.tick(200);
  assert.deepEqual(await observed, { process: "diagnostic-timeout" });
});

test("failure annotation keeps the exact primary Error and never replaces frozen or non-Error failures", async () => {
  const { system } = fixture();
  const original = new Error("original startup failure");
  await annotateOwnedBrowserFailure(original, child(), "/owned/profile", system);
  assert.equal(original.message, "original startup failure");
  assert.equal(original.ownedBrowserStartup.activePort, "valid");
  const frozen = Object.freeze(new Error("immutable primary failure"));
  await annotateOwnedBrowserFailure(frozen, child(), "/owned/profile", system);
  assert.equal(frozen.message, "immutable primary failure");
  assert.equal(frozen.ownedBrowserStartup, undefined);
  const foreign = Object.freeze({ reason: "untrusted non-Error failure" });
  await annotateOwnedBrowserFailure(foreign, child(), "/owned/profile", system);
  assert.deepEqual(foreign, { reason: "untrusted non-Error failure" });
});

test("actual profile reads are bounded, refuse symlink substitution and never print a target", async () => {
  const folder = await mkdtemp(join(tmpdir(), "zoen-startup-diagnostic-"));
  try {
    const profile = join(folder, "profile");
    await mkdir(profile);
    const port = join(profile, "DevToolsActivePort");
    const current = { pid: 42, exitCode: 1, signalCode: null };
    assert.deepEqual(await inspectOwnedBrowserStartup(current, profile), {
      activePort: "absent",
      process: process.platform === "linux" ? "exited" : "unsupported",
    });
    await writeFile(port, active);
    assert.equal((await inspectOwnedBrowserStartup(current, profile)).activePort, "valid");
    await writeFile(port, "a".repeat(129));
    await assert.rejects(boundedDiagnosticFile(port, 128), /exceeds bound/);
    assert.equal((await inspectOwnedBrowserStartup(current, profile)).activePort, "unavailable");
    await rm(port);
    const target = join(folder, "private-canary");
    await writeFile(target, "DO_NOT_READ_PRIVATE_CANARY");
    await symlink(target, port);
    const result = await inspectOwnedBrowserStartup(current, profile);
    assert.equal(result.activePort, "unavailable");
    assert.doesNotMatch(JSON.stringify(result), /CANARY|private/);
    await assert.rejects(boundedDiagnosticFile(profile, 128), /Not a diagnostic file/);
  } finally {
    await rm(folder, { recursive: true, force: true });
  }
});
