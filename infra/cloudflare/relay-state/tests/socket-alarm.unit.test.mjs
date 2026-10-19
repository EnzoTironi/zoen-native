// Deterministic async-storage interleavings; not a workerd alarm/runtime proof.
import assert from "node:assert/strict";
import test from "node:test";
import { SocketAlarm } from "../src/socket-alarm.mjs";
const deferred = () => {
  let resolve;
  const promise = new Promise((accept) => {
    resolve = accept;
  });
  return { promise, resolve };
};
const turn = async () => {
  for (let n = 0; n < 8; n++) await Promise.resolve();
};
function harness({ get, set, remove } = {}) {
  let deadline = 900_000,
    installed = null,
    reads = 0,
    leases = 0;
  const writes = [],
    failures = [];
  const alarm = new SocketAlarm(
    {
      storage: {
        async getAlarm() {
          reads++;
          return get ? get(() => installed) : installed;
        },
        async setAlarm(value) {
          if (set) await set(value);
          installed = value;
          writes.push(value);
        },
        async deleteAlarm() {
          if (remove) await remove();
          installed = null;
          writes.push(null);
        },
      },
      waitUntil(promise) {
        leases++;
        promise.catch((error) => failures.push(error));
      },
    },
    () => deadline,
  );
  return {
    alarm,
    writes,
    failures,
    deadline: (value) => {
      deadline = value;
    },
    installed: (value) => {
      installed = value;
    },
    state: () => ({ installed, reads, leases }),
  };
}
test("a held getAlarm cannot use an old fifteen-minute deadline over new recovery work", async () => {
  const held = deferred();
  let first = true;
  const h = harness({
    get: (read) => {
      if (first) {
        first = false;
        return held.promise;
      }
      return read();
    },
  });
  const original = h.alarm.schedule();
  await turn();
  h.deadline(250);
  assert.equal(h.alarm.schedule(), original);
  held.resolve(900_000);
  await original;
  assert.deepEqual(h.writes, [250]);
  assert.equal(h.state().installed, 250);
  assert.equal(h.state().leases, 1);
});
test("a caller during an old alarm write gets a recomputation before shared settlement", async () => {
  const held = deferred();
  const h = harness({ set: (value) => (value === 900_000 ? held.promise : undefined) });
  const original = h.alarm.schedule();
  await turn();
  h.deadline(250);
  assert.equal(h.alarm.schedule(), original);
  held.resolve();
  await original;
  assert.deepEqual(h.writes, [900_000, 250]);
  assert.equal(h.state().installed, 250);
});
test("a pending stale delete cannot erase newly requested recovery work", async () => {
  const held = deferred();
  const h = harness({ remove: () => held.promise });
  h.installed(900_000);
  h.deadline(null);
  const original = h.alarm.schedule();
  await turn();
  h.deadline(250);
  assert.equal(h.alarm.schedule(), original);
  held.resolve();
  await original;
  assert.deepEqual(h.writes, [null, 250]);
  assert.equal(h.state().installed, 250);
});
test("one thousand concurrent requests share one scheduler lease and bounded state", async () => {
  const held = deferred();
  let first = true;
  const h = harness({
    get: (read) => {
      if (first) {
        first = false;
        return held.promise;
      }
      return read();
    },
  });
  const original = h.alarm.schedule();
  await turn();
  for (let n = 0; n < 1000; n++) assert.equal(h.alarm.schedule(), original);
  h.deadline(250);
  held.resolve(null);
  await original;
  assert.equal(h.state().leases, 1);
  assert.ok(h.state().reads <= 2);
  assert.deepEqual(h.writes, [250]);
});
test("a storage rejection releases the lease without a hot retry and the next actual request retries", async () => {
  let fail = true;
  const h = harness({
    set: () => {
      if (fail) {
        fail = false;
        throw new Error("owned alarm fault");
      }
    },
  });
  await assert.rejects(h.alarm.schedule(), /owned alarm fault/);
  await turn();
  assert.equal(h.state().leases, 1);
  assert.equal(h.state().reads, 1);
  h.deadline(250);
  await h.alarm.schedule();
  assert.equal(h.state().leases, 2);
  assert.deepEqual(h.writes, [250]);
});
