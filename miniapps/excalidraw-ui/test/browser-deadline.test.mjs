import assert from "node:assert/strict";
import test from "node:test";
import { createBrowserDeadline } from "./browser-driver.mjs";
import { withCleanup } from "./browser-lifecycle.mjs";

function pending() {
  let resolve;
  const promise = new Promise((done) => {
    resolve = done;
  });
  return { promise, resolve };
}

function ownedAttempt(deadline, body = new Promise(() => {})) {
  let cleanups = 0;
  return {
    result: withCleanup(
      () => Promise.race([body, deadline.expired]),
      () => {
        deadline.clear();
        cleanups++;
      },
    ),
    cleanups: () => cleanups,
  };
}

test("startup gets its own 30 seconds and still fails closed with cleanup", async (t) => {
  t.mock.timers.enable({ apis: ["setTimeout"] });
  const deadline = createBrowserDeadline();
  const attempt = ownedAttempt(deadline);
  const failed = assert.rejects(attempt.result, /Owned Chrome startup timeout/);
  t.mock.timers.tick(18000);
  await Promise.resolve();
  assert.equal(attempt.cleanups(), 0, "renderer budget cannot expire before Chrome starts");
  t.mock.timers.tick(11999);
  await Promise.resolve();
  assert.equal(attempt.cleanups(), 0);
  t.mock.timers.tick(1);
  await failed;
  assert.equal(attempt.cleanups(), 1);
  deadline.startExecution();
  t.mock.timers.tick(18000);
  assert.equal(attempt.cleanups(), 1, "a late endpoint cannot restart an expired owner");
});

test("slow successful startup leaves the full existing 18-second execution budget", async (t) => {
  t.mock.timers.enable({ apis: ["setTimeout"] });
  const deadline = createBrowserDeadline();
  const attempt = ownedAttempt(deadline);
  const failed = assert.rejects(attempt.result, /Owned Chrome test deadline/);
  t.mock.timers.tick(29999);
  deadline.startExecution();
  t.mock.timers.tick(17999);
  await Promise.resolve();
  assert.equal(attempt.cleanups(), 0, "startup time must not consume execution time");
  t.mock.timers.tick(1);
  await failed;
  assert.equal(attempt.cleanups(), 1);
});

test("repeated DevTools announcements cannot extend the execution deadline", async (t) => {
  t.mock.timers.enable({ apis: ["setTimeout"] });
  const deadline = createBrowserDeadline();
  const attempt = ownedAttempt(deadline);
  const failed = assert.rejects(attempt.result, /Owned Chrome test deadline/);
  t.mock.timers.tick(12000);
  deadline.startExecution();
  t.mock.timers.tick(10000);
  deadline.startExecution();
  t.mock.timers.tick(7999);
  await Promise.resolve();
  assert.equal(attempt.cleanups(), 0);
  t.mock.timers.tick(1);
  await failed;
  assert.equal(attempt.cleanups(), 1);
});

test("success clears both phase budgets before owned cleanup completes", async (t) => {
  t.mock.timers.enable({ apis: ["setTimeout"] });
  const deadline = createBrowserDeadline();
  let expirations = 0;
  void deadline.expired.catch(() => {
    expirations++;
  });
  const body = pending();
  const attempt = ownedAttempt(deadline, body.promise);
  t.mock.timers.tick(24000);
  deadline.startExecution();
  t.mock.timers.tick(17000);
  body.resolve("verified canvas and screenshot");
  assert.equal(await attempt.result, "verified canvas and screenshot");
  assert.equal(attempt.cleanups(), 1);
  t.mock.timers.tick(60000);
  await Promise.resolve();
  assert.equal(expirations, 0, "a completed owner must leave no rejection timer behind");
});
