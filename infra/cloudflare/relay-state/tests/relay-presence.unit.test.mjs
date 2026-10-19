// SOURCE DRAFT: not executed. These are SQLite scheduler/host-hook unit tests,
// not real Auth, hibernation, transport, current-device or native-wire acceptance.
import assert from "node:assert/strict";
import test from "node:test";
import { harness, key, A, B, C, bytes, deferred, microtasks } from "./presence-unit-harness.mjs";

test("registered Auth hook attaches once after Ready, stock precedes async announcement, own snapshot is current", async () => {
  const h = harness();
  try {
    h.add(key(1), A);
    h.add(key(2), B);
    h.registeredAuth(key(2));
    await h.drain();
    h.sent.length = 0;
    h.order.length = 0;
    assert.equal(h.registeredAuth(key(1)), true);
    assert.equal(h.api.attach(key(1)), false);
    await h.drain();
    assert.deepEqual(h.order, ["ready", "stock", "presence", "presence"]);
    assert.deepEqual(h.sent.slice(2), [
      { id: key(2), kind: "presence", identity: A, online: true },
      { id: key(1), kind: "presence", identity: B, online: true },
    ]);
    assert.equal(h.count("relay_attached"), 2);
    assert.equal(h.count("relay_presence_jobs"), 0);
  } finally {
    h.close();
  }
});
test("first Register hook commits then attaches and stocks before Res; repeated Register cannot announce again", async () => {
  const h = harness({ composition: true });
  try {
    const socket = h.add(key(1), A, false);
    assert.equal(h.api.attach(key(1)), false, "Provisional Ready never joins presence");
    h.controller.enqueue(
      socket,
      bytes({
        kind: "request",
        id: "9007199254740993",
        register: true,
        refill: false,
        profile_changed: null,
      }),
    );
    await h.drain();
    assert.deepEqual(h.order, ["stock", "res"]);
    assert.equal(h.count("relay_attached"), 1);
    const calls = h.metrics().coRpcCalls;
    h.controller.enqueue(
      socket,
      bytes({
        kind: "request",
        id: "9007199254740994",
        register: true,
        refill: false,
        profile_changed: null,
      }),
    );
    await h.drain();
    assert.deepEqual(h.order, ["stock", "res", "res"]);
    assert.equal(h.metrics().coRpcCalls, calls);
  } finally {
    h.close();
  }
});
test("each genuine sibling attach announces online but A2 terminal unlink while A1 lives never announces offline", async () => {
  const h = harness();
  try {
    h.add(key(1), A);
    h.add(key(2), A);
    h.add(key(3), B);
    for (const id of [key(1), key(3)]) h.registeredAuth(id);
    await h.drain();
    h.sent.length = 0;
    h.registeredAuth(key(2));
    await h.drain();
    assert.equal(
      h.sent.filter((f) => f.id === key(3) && f.kind === "presence" && f.identity === A && f.online)
        .length,
      1,
    );
    h.sent.length = 0;
    h.retire(key(2));
    h.terminal(key(2));
    await h.drain();
    assert.equal(
      h.sent.some((f) => f.identity === A && f.online === false),
      false,
    );
    assert.equal(h.current(key(1)).identity, A);
    assert.equal(h.row(key(2)), undefined);
  } finally {
    h.close();
  }
});
test("last detach emits offline only after actual terminal event, with no Session reconstruction", async () => {
  const h = harness();
  try {
    h.add(key(1), A);
    h.add(key(2), B);
    h.registeredAuth(key(1));
    h.registeredAuth(key(2));
    await h.drain();
    h.sent.length = 0;
    h.retire(key(1));
    await h.drain();
    assert.deepEqual(h.sent, []);
    assert.equal(h.row(key(1)).stage, "closing");
    h.terminal(key(1));
    await h.drain();
    assert.deepEqual(h.sent, [{ id: key(2), kind: "presence", identity: A, online: false }]);
    assert.equal(h.row(key(1)), undefined);
  } finally {
    h.close();
  }
});
test("new same-identity connection during offline audience await suppresses stale false", async () => {
  const gate = deferred();
  let pause = false;
  const h = harness({
    audience: (who) => (pause && who === A ? gate.promise : [A, B].filter((id) => id !== who)),
  });
  try {
    h.add(key(1), A);
    h.add(key(2), B);
    h.registeredAuth(key(1));
    h.registeredAuth(key(2));
    await h.drain();
    h.sent.length = 0;
    pause = true;
    h.retire(key(1));
    h.terminal(key(1));
    await microtasks();
    h.add(key(3), A);
    h.api.attach(key(3));
    gate.resolve([B]);
    await h.drain();
    assert.equal(
      h.sent.some((f) => f.identity === A && f.online === false),
      false,
    );
    assert.ok(h.current(key(3)));
  } finally {
    gate.resolve([B]);
    await h.drain();
    h.close();
  }
});
test("source retirement while online lookup waits cannot reattach or emit stale true", async () => {
  const gate = deferred();
  const h = harness({ audience: (who) => (who === A ? gate.promise : [A]) });
  try {
    h.add(key(1), A);
    h.add(key(2), B);
    h.registeredAuth(key(2));
    await h.drain();
    h.sent.length = 0;
    h.api.attach(key(1));
    await microtasks();
    h.retire(key(1));
    h.terminal(key(1));
    gate.resolve([B]);
    await h.drain();
    assert.equal(
      h.sent.some((f) => f.identity === A && f.online === true),
      false,
    );
    assert.equal(h.current(key(1)), null);
  } finally {
    gate.resolve([B]);
    await h.drain();
    h.close();
  }
});
test("duplicate terminal callback preserves one pending offline source while its audience waits", async () => {
  const gate = deferred();
  let pause = false;
  const h = harness({
    audience: (who) => (pause && who === A ? gate.promise : [A, B].filter((id) => id !== who)),
  });
  try {
    h.add(key(1), A);
    h.add(key(2), B);
    h.registeredAuth(key(1));
    h.registeredAuth(key(2));
    await h.drain();
    h.sent.length = 0;
    pause = true;
    h.retire(key(1));
    h.terminal(key(1));
    await microtasks();
    h.api.ended(key(1));
    assert.equal(h.row(key(1)).stage, "ended");
    assert.equal(h.count("relay_presence_jobs"), 1);
    gate.resolve([B]);
    await h.drain();
    assert.equal(h.sent.filter((f) => f.identity === A && f.online === false).length, 1);
    assert.equal(h.row(key(1)), undefined);
  } finally {
    gate.resolve([B]);
    await h.drain();
    h.close();
  }
});
test("row cleanup failure keeps a bounded intent and releases the running slot without an unhandled wake loop", async () => {
  const h = harness();
  try {
    h.add(key(1), A);
    h.db.exec(
      "CREATE TRIGGER unit_delete_fault BEFORE DELETE ON relay_presence_jobs BEGIN SELECT RAISE(ABORT,'unit cleanup failure'); END",
    );
    h.api.attach(key(1));
    await h.drain();
    assert.equal(h.count("relay_presence_jobs"), 1);
    assert.deepEqual(h.failures, []);
    h.db.exec("DROP TRIGGER unit_delete_fault");
    h.api.kick();
    await h.drain();
    assert.equal(h.count("relay_presence_jobs"), 0);
  } finally {
    h.close();
  }
});
test("cold reconstruction preserves pending actual attach/job state but never manufactures a new attach", async () => {
  const h = harness({ wakeFault: true });
  try {
    h.add(key(1), A);
    h.api.attach(key(1));
    await h.drain();
    assert.equal(h.count("relay_presence_jobs"), 1);
    assert.deepEqual(h.sent, []);
    h.setWakeFault(false);
    const cold = h.reconstruct();
    assert.equal(cold.pending(), true);
    assert.equal(h.count("relay_attached"), 1);
    assert.deepEqual(h.sent, []);
    cold.kick();
    await h.drain();
    assert.equal(h.count("relay_attached"), 1);
    assert.equal(h.count("relay_presence_jobs"), 0);
    assert.equal(h.metrics().coCalls, 1);
  } finally {
    h.close();
  }
});
test("cold terminal callback consumes an existing attach and never resurrects an ended generation", async () => {
  const h = harness({ wakeFault: true });
  try {
    h.add(key(1), A);
    h.add(key(2), B);
    h.api.attach(key(1));
    h.api.attach(key(2));
    await h.drain();
    h.retire(key(1));
    const cold = h.reconstruct();
    h.setWakeFault(false);
    h.terminal(key(1));
    await h.drain();
    assert.equal(h.current(key(1)), null);
    assert.equal(h.row(key(1)), undefined);
    assert.equal(h.count("relay_attached"), 1);
    assert.equal(cold.isAttached(key(1)), false);
    assert.ok(h.sent.some((f) => f.id === key(2) && f.identity === A && f.online === false));
  } finally {
    h.close();
  }
});
test("missing wake stays bounded and is retried only on a later real alarm/event", async () => {
  const h = harness({ wakeFault: true });
  try {
    h.add(key(1), A);
    h.api.attach(key(1));
    await h.drain();
    assert.equal(h.metrics().wakes, 1);
    assert.equal(h.metrics().coCalls, 0);
    assert.equal(h.count("relay_presence_jobs"), 1);
    h.setWakeFault(false);
    h.api.kick();
    await h.drain();
    assert.equal(h.count("relay_presence_jobs"), 0);
    assert.deepEqual(h.failures, []);
  } finally {
    h.close();
  }
});
test("separate eight-slot lease survives caller timeouts and leaves ordinary foreground headroom", async () => {
  const held = deferred();
  const h = harness({
    composition: true,
    rpc: async ({ identity }) => (identity === C ? { members: [] } : held.promise),
  });
  try {
    for (let n = 1; n <= 4; n++) {
      const who = n % 2 ? A : B;
      h.add(key(n), who);
      h.discover(who);
      h.api.attach(key(n));
    }
    await microtasks();
    assert.equal(h.metrics().coRpcCalls, 8);
    assert.equal(h.metrics().activeRpc, 8);
    await new Promise((resolve) => setTimeout(resolve, 2100));
    await h.drain();
    assert.equal(h.metrics().activeRpc, 8, "Caller timeout does not settle real RPCs");
    assert.equal(
      h.metrics().coRpcCalls,
      8,
      "Later queued presence jobs cannot exceed eight unsettled operations",
    );
    const foreground = h.add(key(9), C);
    h.discover(C);
    h.controller.enqueue(
      foreground,
      bytes({
        kind: "request",
        id: "99",
        register: false,
        refill: false,
        profile_changed: { identity: C, version: "1" },
      }),
    );
    await h.drain();
    assert.ok(h.metrics().coRpcCalls > 8, "Ordinary profile union still has global RPC headroom");
    assert.equal(h.metrics().activeRpc, 8);
    held.resolve({ members: [C] });
    await microtasks();
    assert.equal(h.metrics().activeRpc, 0);
    h.add(key(10), A);
    h.api.attach(key(10));
    await h.drain();
    assert.equal(h.metrics().activeRpc, 0);
    assert.deepEqual(h.failures, []);
  } finally {
    held.resolve({ members: [] });
    await h.drain();
    h.close();
  }
});

test("bounded queue and attachment capacity cannot corrupt registration or evict earlier generations", async () => {
  const h = harness({ wakeFault: true });
  try {
    // Unit-only server-generation backlog. No forged session/profile/device facts.
    const insert = h.db.prepare(
      "INSERT INTO relay_presence_jobs(connection_id,kind) VALUES (?,'online')",
    );
    for (let n = 0; n < 512; n++) insert.run(key(2000 + n));
    for (let n = 0; n < 256; n++) {
      const id = key(3000 + n);
      h.add(id, A);
      assert.equal(h.api.attach(id), true);
    }
    const overflow = key(4000);
    h.add(overflow, A);
    assert.equal(h.api.attach(overflow), false);
    await h.drain();
    assert.equal(h.count("relay_presence_jobs"), 512);
    assert.equal(h.count("relay_attached"), 256);
    assert.equal(
      h.row(overflow).registered,
      1,
      "Best-effort capacity never rolls back actual registration",
    );
    assert.equal(h.current(key(3000)).identity, A);
    assert.deepEqual(h.failures, []);
  } finally {
    h.close();
  }
});
test("terminal queue write fault rolls back bookkeeping and never restores active authority", async () => {
  const h = harness();
  try {
    h.add(key(1), A);
    h.add(key(2), B);
    h.registeredAuth(key(1));
    h.registeredAuth(key(2));
    await h.drain();
    h.sent.length = 0;
    h.retire(key(1));
    h.db.exec(
      "CREATE TRIGGER unit_offline_fault BEFORE INSERT ON relay_presence_jobs WHEN NEW.kind='offline' BEGIN SELECT RAISE(ABORT,'unit offline queue failure'); END",
    );
    assert.throws(() => h.terminal(key(1)), /unit offline queue failure/);
    assert.equal(h.current(key(1)), null);
    assert.equal(h.row(key(1)).stage, "ended");
    assert.equal(h.count("relay_attached"), 2);
    assert.equal(h.api.pending(), true, "Recorded ended generation keeps the shared alarm alive");
    assert.deepEqual(h.sent, []);
    h.db.exec("DROP TRIGGER unit_offline_fault");
    h.api.kick();
    await h.drain();
    assert.equal(h.row(key(1)), undefined);
    assert.equal(h.sent.filter((f) => f.identity === A && f.online === false).length, 1);
  } finally {
    h.close();
  }
});
test("unrelated or removed audience and an invalid partial roster cannot receive any notice", async () => {
  let response = [];
  const h = harness({ audience: () => response });
  try {
    h.add(key(1), A);
    h.add(key(2), B);
    h.add(key(3), C);
    h.registeredAuth(key(2));
    h.registeredAuth(key(3));
    await h.drain();
    h.sent.length = 0;
    h.api.attach(key(1));
    await h.drain();
    assert.deepEqual(h.sent, [], "An empty current audience grants nobody");
    h.retire(key(1));
    h.terminal(key(1));
    await h.drain();
    h.add(key(4), A);
    response = [B, "invalid-identity"];
    h.api.attach(key(4));
    await h.drain();
    assert.deepEqual(h.sent, [], "Validate entire union before sending to even its valid prefix");
  } finally {
    h.close();
  }
});

test("a recipient retired during awaited lookup cannot receive stale presence or snapshot", async () => {
  const gate = deferred();
  let pause = false;
  const h = harness({
    audience: (who) => (pause && who === A ? gate.promise : [A, B].filter((id) => id !== who)),
  });
  try {
    h.add(key(1), A);
    h.add(key(2), B);
    h.registeredAuth(key(2));
    await h.drain();
    h.sent.length = 0;
    pause = true;
    h.api.attach(key(1));
    await microtasks();
    h.retire(key(2));
    gate.resolve([B]);
    await h.drain();
    assert.deepEqual(h.sent, []);
    assert.equal(h.current(key(2)), null);
  } finally {
    gate.resolve([B]);
    await h.drain();
    h.close();
  }
});
test("job generations above JS safe integer remain exact text through scheduling and cleanup", async () => {
  const h = harness({ wakeFault: true });
  try {
    h.db.exec(
      "INSERT INTO sqlite_sequence(name,seq) VALUES ('relay_presence_jobs',9007199254740992)",
    );
    h.add(key(1), A);
    h.api.attach(key(1));
    await h.drain();
    const queued = h.db.prepare("SELECT CAST(id AS TEXT) AS id FROM relay_presence_jobs").get();
    assert.equal(queued.id, "9007199254740993");
    h.setWakeFault(false);
    h.api.kick();
    await h.drain();
    assert.equal(h.count("relay_presence_jobs"), 0);
    assert.deepEqual(h.failures, []);
  } finally {
    h.close();
  }
});

test("ended-row cleanup fault retains its job atomically for a later alarm without resurrecting auth", async () => {
  const h = harness();
  try {
    h.add(key(1), A);
    h.add(key(2), B);
    h.registeredAuth(key(1));
    h.registeredAuth(key(2));
    await h.drain();
    h.sent.length = 0;
    h.retire(key(1));
    h.db.exec(
      "CREATE TRIGGER unit_ended_delete_fault BEFORE DELETE ON unit_connections WHEN OLD.stage='ended' BEGIN SELECT RAISE(ABORT,'unit ended cleanup failure'); END",
    );
    h.terminal(key(1));
    await h.drain();
    assert.equal(h.current(key(1)), null);
    assert.equal(h.row(key(1)).stage, "ended");
    assert.equal(h.count("relay_presence_jobs"), 1);
    assert.deepEqual(h.failures, []);
    h.db.exec("DROP TRIGGER unit_ended_delete_fault");
    h.api.kick();
    await h.drain();
    assert.equal(h.row(key(1)), undefined);
    assert.equal(h.count("relay_presence_jobs"), 0);
    assert.equal(h.current(key(1)), null);
  } finally {
    h.close();
  }
});

test("stopped transports lose old live authority and reconnect requires a new genuine generation attach", async () => {
  const h = harness();
  try {
    h.add(key(1), A);
    h.add(key(2), B);
    h.registeredAuth(key(1));
    h.registeredAuth(key(2));
    await h.drain();
    h.sent.length = 0;
    // Host-double stop: every old physical socket has ended. The helper must not
    // manufacture an offline/online event or rebuild a live Session from SQL rows.
    for (const socket of h.ctx.getWebSockets()) socket.readyState = 3;
    const before = h.metrics().coCalls;
    h.reconstruct();
    h.api.kick();
    await h.drain();
    assert.equal(h.current(key(1)), null);
    assert.equal(h.current(key(2)), null);
    assert.deepEqual(h.sent, []);
    assert.equal(h.metrics().coCalls, before);
    h.add(key(3), A);
    h.add(key(4), B);
    h.registeredAuth(key(3));
    h.registeredAuth(key(4));
    await h.drain();
    assert.ok(h.sent.some((f) => f.kind === "presence" && f.online));
    assert.ok(h.sent.every((f) => f.id === key(3) || f.id === key(4)));
    assert.equal(h.current(key(1)), null);
    assert.equal(h.current(key(2)), null);
  } finally {
    h.close();
  }
});
