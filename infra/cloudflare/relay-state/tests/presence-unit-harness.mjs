// UNIT DOUBLES ONLY. Real SQLite, production draft scheduler/composition; the
// host registry deliberately models current Directory hooks. This does not prove
// cryptographic Auth, actual workerd hibernation/transport close or native wire.
import assert from "node:assert/strict";
import { DatabaseSync } from "node:sqlite";
import { RelayPresence } from "../src/relay-presence.mjs";
import { RelayComposition } from "../src/relay-composition.mjs";
export const key = (n) => n.toString(16).padStart(64, "0");
export const A = key(1001),
  B = key(1002),
  C = key(1003);
export const bytes = (value) => new TextEncoder().encode(JSON.stringify(value));
export const value = (bytes) => JSON.parse(new TextDecoder().decode(bytes));
export const deferred = () => {
  let resolve, reject;
  const promise = new Promise((a, b) => {
    resolve = a;
    reject = b;
  });
  return { promise, resolve, reject };
};
export async function microtasks() {
  for (let n = 0; n < 40; n++) await Promise.resolve();
}

export function harness({
  composition = false,
  wakeFault = false,
  audience = null,
  rpc = null,
} = {}) {
  const db = new DatabaseSync(":memory:");
  db.exec(
    "CREATE TABLE unit_connections(id TEXT PRIMARY KEY,identity TEXT NOT NULL,registered INTEGER NOT NULL,stage TEXT NOT NULL)",
  );
  const sockets = new Map(),
    pending = [],
    sent = [],
    failures = [],
    order = [];
  let fault = wakeFault,
    wakes = 0,
    coCalls = 0,
    coRpcCalls = 0,
    activeRpc = 0,
    maximumRpc = 0;
  const rows = (query, ...args) => {
    const result = db.prepare(query).all(...args);
    return Object.assign(result, { toArray: () => Array.from(result) });
  };
  const row = (id) => db.prepare("SELECT * FROM unit_connections WHERE id=?").get(id);
  const current = (id) => {
    const actual = row(id),
      socket = sockets.get(id);
    return actual?.stage === "ready" && actual.registered === 1 && socket?.readyState === 1
      ? { id, identity: actual.identity, socket }
      : null;
  };
  const active = () =>
    [...sockets.keys()].flatMap((id) => {
      const ready = current(id);
      return ready ? [ready] : [];
    });
  const peers = (who) => [A, B, C].filter((id) => id !== who);
  const ctx = {
    storage: {
      sql: { exec: rows },
      transactionSync(work) {
        db.exec("BEGIN");
        try {
          const result = work();
          assert.equal(result instanceof Promise, false, "No SQL transaction crosses await");
          db.exec("COMMIT");
          return result;
        } catch (error) {
          db.exec("ROLLBACK");
          throw error;
        }
      },
    },
    getWebSockets: () => [...sockets.values()],
    waitUntil(promise) {
      pending.push(
        Promise.resolve(promise).catch((error) => {
          failures.push(error);
          throw error;
        }),
      );
    },
  };
  const send = (socket, frame) => {
    const fresh = current(socket.id);
    if (!fresh) return false;
    sent.push({ id: socket.id, ...value(frame) });
    order.push(value(frame).kind);
    return true;
  };
  const host = {
    sockets: () => [...sockets.values()],
    current,
    active,
    ended: (id) => {
      const actual = row(id);
      return actual?.stage === "ended" ? { id, identity: actual.identity } : null;
    },
    discardEnded: (id) =>
      db.prepare("DELETE FROM unit_connections WHERE id=? AND stage='ended'").run(id),
    coMembers: async (who) => {
      coCalls++;
      return audience ? audience(who) : peers(who);
    },
    control: (notice) => bytes({ kind: "presence", ...notice }),
    send,
    wake: async () => {
      wakes++;
      if (fault) throw new Error("unit alarm installation failure");
    },
  };
  const spaces = {
    async coMembers(input) {
      coRpcCalls++;
      activeRpc++;
      maximumRpc = Math.max(maximumRpc, activeRpc);
      try {
        return rpc ? await rpc(input) : { members: peers(input.identity) };
      } finally {
        activeRpc--;
      }
    },
  };
  let controller;
  if (composition)
    controller = new RelayComposition(ctx, spaces, {
      sockets: () => [...sockets.values()],
      presence: host,
      socketId: (socket) => socket.id,
      socketFor: (id) => current(id)?.socket ?? sockets.get(id),
      socketRow: (socket) => row(socket.id),
      incoming: (raw) => value(raw),
      admit: (id) => {
        const actual = row(id);
        return {
          state: "admitted",
          registered: actual.registered === 1,
          identity: actual.identity,
          origin_connection: id,
          session: { authenticated_identity: actual.identity, authenticated_device: key(999) },
        };
      },
      request(admitted) {
        db.prepare("UPDATE unit_connections SET registered=1 WHERE id=?").run(
          admitted.origin_connection,
        );
        return bytes({ kind: "res", ok: true });
      },
      effects: (raw) => ({ ok: value(raw).ok, claimed: [] }),
      ownStock(socket) {
        assert.equal(
          controller.presenceAttached(socket.id),
          true,
          "Attach precedes stock query/notice",
        );
        send(socket, bytes({ kind: "stock" }));
      },
      claimedStock: () => assert.fail("not a claim"),
      control: (input) => bytes(input),
      send,
      wake: host.wake,
      reason: (error) => ({ reason: error.message }),
      close: () => assert.fail("presence must not fail registration"),
    });
  else controller = new RelayPresence(ctx, host);
  const api = composition
    ? {
        attach: (id) => controller.attached(id),
        closing: (id) => controller.presenceClosing(id),
        ended: (id) => controller.presenceEnded(id),
        kick: () => controller.kick(),
        pending: () => controller.pending(),
      }
    : {
        attach: (id) => controller.attach(id),
        closing: (id) => controller.closing(id),
        ended: (id) => controller.transportEnded(id),
        kick: () => controller.kick(),
        pending: () => controller.pending(),
      };
  return {
    db,
    ctx,
    host,
    sent,
    order,
    failures,
    controller,
    api,
    row,
    current,
    count: (table) => db.prepare(`SELECT count(*) AS n FROM ${table}`).get().n,
    add(id, identity, registered = true) {
      const socket = { id, readyState: 1 };
      sockets.set(id, socket);
      db.prepare("INSERT INTO unit_connections VALUES (?,?,?,'ready')").run(
        id,
        identity,
        Number(registered),
      );
      return socket;
    },
    registeredAuth(id) {
      send(sockets.get(id), bytes({ kind: "ready" }));
      const attached = api.attach(id);
      if (attached) {
        assert.equal(this.count("relay_attached") > 0, true);
        send(sockets.get(id), bytes({ kind: "stock" }));
      }
      return attached;
    },
    retire(id) {
      db.prepare("UPDATE unit_connections SET stage='closing' WHERE id=?").run(id);
      api.closing(id);
    },
    terminal(id) {
      sockets.get(id).readyState = 3;
      db.prepare("UPDATE unit_connections SET stage='ended' WHERE id=?").run(id);
      api.ended(id);
    },
    setWakeFault(value) {
      fault = value;
    },
    reconstruct() {
      assert.equal(composition, false);
      const fresh = new RelayPresence(ctx, host);
      this.controller = fresh;
      for (const [name, fn] of Object.entries({
        attach: (id) => fresh.attach(id),
        closing: (id) => fresh.closing(id),
        ended: (id) => fresh.transportEnded(id),
        kick: () => fresh.kick(),
        pending: () => fresh.pending(),
      }))
        api[name] = fn;
      return fresh;
    },
    discover(identity, count = 4) {
      for (let n = 0; n < count; n++)
        db.prepare("INSERT OR IGNORE INTO relay_discovery VALUES (?,?)").run(
          identity,
          `sp_unit_${n}`,
        );
    },
    async drain() {
      for (let n = 0; n < pending.length; n++) {
        assert.ok(n < 2048, "Bounded task scheduling");
        await pending[n];
      }
      await microtasks();
    },
    metrics: () => ({ wakes, coCalls, coRpcCalls, activeRpc, maximumRpc }),
    close() {
      db.close();
    },
  };
}
