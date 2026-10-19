// Control-plane fault tests with real SQLite transactions and real Rust/WASM
// admission over the signed native fixture. This is not Directory/socket auth
// acceptance; the separate actual wire journey must supply that public proof.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { DatabaseSync } from "node:sqlite";
import test from "node:test";
import { RelayPublications } from "../src/relay-publications.mjs";
import {
  initSync,
  relayPublicationMetadata,
  closedRelayPreview,
  closedPublish,
  closedReplay,
} from "../generated/wasm/admission.js";

initSync({
  module: await readFile(new URL("../generated/wasm/admission_bg.wasm", import.meta.url)),
});
const native = JSON.parse(
  await readFile(
    process.env.ZOEN_CLOSED_FIXTURE ?? new URL("../generated/closed-fixture.json", import.meta.url),
    "utf8",
  ),
);
const genesis = native.steps.find(
  (step) =>
    step.op === "publish" && step.expected.kind === "new" && step.expected.event.seq === "0",
);
assert.ok(genesis);
const signed = Uint8Array.from(Buffer.from(genesis.frame_base64, "base64"));
const deferred = () => {
  let resolve, reject;
  const promise = new Promise((a, b) => {
    resolve = a;
    reject = b;
  });
  return { promise, resolve, reject };
};

function harness(dispatch = async () => ({ acknowledged: true })) {
  const db = new DatabaseSync(":memory:");
  db.exec(
    "CREATE TABLE unit_projection(singleton INTEGER PRIMARY KEY,bytes TEXT NOT NULL); CREATE TABLE unit_events(seq TEXT PRIMARY KEY,metadata TEXT NOT NULL,bytes BLOB NOT NULL)",
  );
  const rows = (query, ...args) => {
    const result = db.prepare(query).all(...args);
    return Object.assign(result, { toArray: () => Array.from(result) });
  };
  let inTransaction = false,
    clock = Number(genesis.now_ms),
    alarm = null;
  const pending = [];
  const callback = [];
  const ctx = {
    storage: {
      sql: { exec: rows },
      transactionSync(fn) {
        assert.equal(inTransaction, false, "No nested SQL transaction");
        inTransaction = true;
        db.exec("BEGIN");
        try {
          const result = fn();
          assert.equal(result instanceof Promise, false);
          db.exec("COMMIT");
          return result;
        } catch (error) {
          db.exec("ROLLBACK");
          throw error;
        } finally {
          inTransaction = false;
        }
      },
      async setAlarm(value) {
        alarm = value;
      },
      async deleteAlarm() {
        alarm = null;
      },
    },
    waitUntil(promise) {
      pending.push(promise);
    },
  };
  const projection = () => {
    const row = db.prepare("SELECT bytes FROM unit_projection WHERE singleton=1").get();
    return row ? JSON.parse(row.bytes) : null;
  };
  const event = () => db.prepare("SELECT metadata,bytes FROM unit_events WHERE seq='0'").get();
  const context = (input) => {
    const state = projection();
    return {
      session: input.session,
      space_id: native.space_id,
      projection: state,
      history: state?.head ? [state.head] : [],
      target: input.target ?? null,
      invite: null,
      now_ms: String(clock),
      prune_ceiling_secs: null,
    };
  };
  const host = {
    nowMs: () => String(clock),
    dependencies() {},
    verify: (input) =>
      JSON.parse(relayPublicationMetadata(input.frame, JSON.stringify(input.session))),
    preview: (input) =>
      JSON.parse(
        closedRelayPreview(
          input.frame,
          JSON.stringify(context(input)),
          event()?.bytes ?? new Uint8Array(),
        ),
      ),
    snapshot: () => ({ head: projection()?.head ?? null }),
    event: () => {
      const row = event();
      return row ? { ...JSON.parse(row.metadata), bytes: new Uint8Array(row.bytes) } : null;
    },
    publish(input) {
      let result;
      const existing = event();
      if (existing)
        result = JSON.parse(
          closedReplay(
            input.frame,
            JSON.stringify(input.session),
            JSON.stringify(projection()),
            existing.bytes,
          ),
        );
      else {
        result = JSON.parse(closedPublish(input.frame, JSON.stringify(context(input))));
        const { bytes_base64, ...metadata } = result.event;
        db.prepare("INSERT INTO unit_events VALUES (?,?,?)").run(
          metadata.seq,
          JSON.stringify(metadata),
          Buffer.from(bytes_base64, "base64"),
        );
        db.prepare("INSERT INTO unit_projection VALUES (1,?)").run(
          JSON.stringify(result.projection),
        );
      }
      return { result };
    },
    async dispatch(packet) {
      assert.equal(inTransaction, false, "No SQL transaction crosses Directory RPC");
      callback.push(packet);
      return dispatch(packet);
    },
  };
  const control = new RelayPublications(ctx, host);
  const input = (n = 1) => ({
    frame: signed,
    session: genesis.session,
    target: null,
    origin_connection: n.toString(16).padStart(64, "0"),
  });
  const catalogue = (prepared) => ({
    preparation_id: prepared.preparation_id,
    space_id: prepared.space_id,
    frame_hash: prepared.frame_hash,
    identities: prepared.grants,
  });
  const commit = (prepared, request = input()) =>
    control.commit({
      ...request,
      preparation_id: prepared.preparation_id,
      catalogue: catalogue(prepared),
    });
  return {
    db,
    ctx,
    host,
    control,
    input,
    catalogue,
    commit,
    callback,
    pending,
    count: () => db.prepare("SELECT count(*) AS n FROM unit_events").get().n,
    alarm: () => alarm,
    advance: (ms) => {
      clock += ms;
    },
    fireAlarm: () => {
      alarm = null;
      return control.alarm();
    },
    close: async () => {
      await Promise.all(pending);
      db.close();
    },
  };
}

test("discovery preparation precedes actual append and exact catalogue coverage is mandatory", async () => {
  const h = harness();
  try {
    const prepared = await h.control.prepare(h.input());
    assert.equal(prepared.state, "prepared");
    assert.equal(h.count(), 0);
    assert.ok(h.alarm());
    assert.deepEqual(prepared.grants, [genesis.session.authenticated_identity]);
    const denied = await h.control.commit({
      ...h.input(),
      preparation_id: prepared.preparation_id,
      catalogue: { ...h.catalogue(prepared), identities: [] },
    });
    assert.equal(denied.state, "rejected");
    assert.equal(h.count(), 0);
    const result = await h.commit(prepared);
    assert.equal(result.state, "committed");
    assert.equal(result.dispatch, "acked");
    assert.equal(h.count(), 1);
    assert.equal(h.callback.length, 1);
    assert.equal(h.callback[0].event.hash, genesis.expected.event.hash);
    assert.deepEqual(
      Buffer.from(h.callback[0].event.bytes),
      Buffer.from(genesis.expected.event.bytes_base64, "base64"),
    );
    await assert.rejects(
      () => h.commit(prepared),
      /relay_preparation_unavailable/,
      "GC after ack is ambiguous, never false Rejected",
    );
  } finally {
    await h.close();
  }
});

test("grantset is re-admitted against current state and CID retry remains canonical", async () => {
  const h = harness();
  try {
    const a = await h.control.prepare(h.input(1));
    const b = await h.control.prepare(h.input(2));
    assert.equal((await h.commit(b, h.input(2))).dispatch, "acked");
    const stale = await h.commit(a, h.input(1));
    assert.equal(stale.state, "rejected");
    assert.match(stale.reason, /grants changed/);
    assert.equal(h.count(), 1);
    const retry = await h.control.prepare(h.input(1));
    assert.deepEqual(retry.grants, []);
    const duplicate = await h.commit(retry, h.input(1));
    assert.equal(duplicate.dispatch, "acked");
    assert.equal(h.count(), 1);
    assert.equal(h.callback.at(-1).kind, "duplicate");
  } finally {
    await h.close();
  }
});

test("late outbox SQL failure rolls the actual canonical append and projection back", async () => {
  const h = harness();
  try {
    const prepared = await h.control.prepare(h.input());
    h.db.exec(
      "CREATE TRIGGER reject_outbox BEFORE INSERT ON relay_publication_outbox BEGIN SELECT RAISE(ABORT,'late outbox write failure'); END",
    );
    await assert.rejects(() => h.commit(prepared), /late outbox write failure/);
    assert.equal(h.count(), 0);
    assert.equal(h.db.prepare("SELECT count(*) AS n FROM unit_projection").get().n, 0);
    assert.equal(h.callback.length, 0);
    h.db.exec("DROP TRIGGER reject_outbox");
    assert.equal((await h.commit(prepared)).dispatch, "acked");
    assert.equal(h.count(), 1);
  } finally {
    await h.close();
  }
});

test("lost callback acknowledgement retains the committed reference and exact retry replays canonically", async () => {
  let attempts = 0;
  const h = harness(async () => {
    if (++attempts === 1) throw new Error("Acknowledgement lost after receiver observed delivery");
    return { acknowledged: true };
  });
  try {
    const prepared = await h.control.prepare(h.input());
    const first = await h.commit(prepared);
    assert.equal(first.state, "committed");
    assert.equal(first.dispatch, "pending");
    assert.equal(h.count(), 1);
    assert.equal(h.callback.length, 1);
    assert.equal(h.db.prepare("SELECT count(*) AS n FROM relay_publication_outbox").get().n, 1);
    assert.ok(h.alarm());
    const retry = await h.commit(prepared);
    assert.equal(retry.dispatch, "acked");
    assert.equal(retry.dispatch_ordinal, first.dispatch_ordinal);
    assert.equal(h.count(), 1);
    assert.equal(h.callback.length, 2);
    assert.deepEqual(h.callback[1], h.callback[0]);
    await assert.rejects(() => h.commit(prepared), /relay_preparation_unavailable/);
    const reconnect = await h.control.prepare(h.input(2));
    assert.deepEqual(reconnect.grants, []);
    assert.equal((await h.commit(reconnect, h.input(2))).dispatch, "acked");
    assert.equal(h.count(), 1);
    assert.equal(h.callback.at(-1).kind, "duplicate");
    assert.equal(h.callback.at(-1).event.hash, h.callback[0].event.hash);
    assert.deepEqual(h.callback.at(-1).event.bytes, h.callback[0].event.bytes);
  } finally {
    await h.close();
  }
});

test("a restarted control instance resumes only the durable canonical reference after acknowledgement loss", async () => {
  let attempts = 0;
  const h = harness(async () => {
    if (++attempts === 1) throw new Error("Receiver acknowledgement was lost");
    return { acknowledged: true };
  });
  try {
    const prepared = await h.control.prepare(h.input());
    assert.equal((await h.commit(prepared)).dispatch, "pending");
    const restarted = new RelayPublications(h.ctx, h.host);
    await restarted.alarm();
    await Promise.all(h.pending);
    assert.equal(h.count(), 1);
    assert.equal(h.callback.length, 2);
    assert.deepEqual(h.callback[1], h.callback[0]);
    assert.equal(h.db.prepare("SELECT count(*) AS n FROM relay_publication_outbox").get().n, 0);
    await assert.rejects(
      () =>
        restarted.commit({
          ...h.input(),
          preparation_id: prepared.preparation_id,
          catalogue: h.catalogue(prepared),
        }),
      /relay_preparation_unavailable/,
    );
  } finally {
    await h.close();
  }
});

test("failed durable preparation wake cannot return a ticket or append a canonical event", async () => {
  const h = harness();
  try {
    const schedule = h.ctx.storage.setAlarm;
    h.ctx.storage.setAlarm = async () => {
      throw new Error("Durable wake write failed");
    };
    await assert.rejects(() => h.control.prepare(h.input()), /Durable wake write failed/);
    assert.equal(h.count(), 0);
    assert.equal(h.callback.length, 0);
    h.ctx.storage.setAlarm = schedule;
    const prepared = await h.control.prepare(h.input());
    assert.equal(prepared.state, "prepared");
    assert.ok(h.alarm());
    assert.equal((await h.commit(prepared)).dispatch, "acked");
    assert.equal(h.count(), 1);
  } finally {
    await h.close();
  }
});

test("one hung callback retains its lease and consumed alarm is renewed without stacking callbacks", async (t) => {
  const started = deferred(),
    release = deferred();
  const h = harness(async () => {
    started.resolve();
    return release.promise;
  });
  t.mock.timers.enable({ apis: ["setTimeout"] });
  try {
    const prepared = await h.control.prepare(h.input());
    const first = h.commit(prepared);
    await started.promise;
    t.mock.timers.tick(4000);
    assert.equal((await first).dispatch, "pending");
    assert.equal(h.callback.length, 1);
    assert.equal((await h.commit(prepared)).dispatch, "pending");
    assert.equal(h.callback.length, 1);
    await h.fireAlarm();
    assert.ok(h.alarm(), "A consumed alarm is durably renewed while the RPC is unresolved");
    assert.equal(h.callback.length, 1);
    release.resolve({ acknowledged: true });
    await Promise.all(h.pending);
    assert.equal(h.count(), 1);
    assert.equal(h.db.prepare("SELECT count(*) AS n FROM relay_publication_outbox").get().n, 0);
  } finally {
    release.resolve({ acknowledged: true });
    t.mock.timers.reset();
    await h.close();
  }
});

test("expired uncommitted tickets and nonempty untracked Spaces fail closed", async () => {
  const h = harness();
  try {
    const prepared = await h.control.prepare(h.input());
    h.advance(30000);
    const expired = await h.commit(prepared);
    assert.equal(expired.state, "rejected");
    assert.match(expired.reason, /expired/);
    assert.equal(h.count(), 0);
  } finally {
    await h.close();
  }
  const legacy = harness();
  try {
    legacy.host.publish(legacy.input());
    assert.equal(legacy.count(), 1);
    const denied = await legacy.control.prepare(legacy.input());
    assert.equal(denied.state, "rejected");
    assert.match(denied.reason, /nonempty precomposition/);
    assert.equal(legacy.db.prepare("SELECT count(*) AS n FROM relay_space_mode").get().n, 0);
    assert.equal(legacy.count(), 1);
  } finally {
    await legacy.close();
  }
});

test("isolate callback lease bounds multiple Spaces before reading or retaining more frames", async (t) => {
  const release = deferred();
  const all = Array.from({ length: 5 }, () => harness(async () => release.promise));
  t.mock.timers.enable({ apis: ["setTimeout"] });
  try {
    const operations = [];
    for (const h of all) {
      const prepared = await h.control.prepare(h.input());
      operations.push(h.commit(prepared));
    }
    // Four callbacks retain their actual lease; the fifth stores its intent
    // without retaining another canonical event or a queue of budget waiters.
    assert.equal(
      all.reduce((n, h) => n + h.callback.length, 0),
      4,
    );
    t.mock.timers.tick(4000);
    const results = await Promise.all(operations);
    assert.ok(results.every((result) => result.dispatch === "pending"));
    release.resolve({ acknowledged: true });
    await Promise.all(all.flatMap((h) => h.pending));
    await all[4].fireAlarm();
    await Promise.all(all[4].pending);
    assert.equal(all[4].callback.length, 1);
    assert.ok(
      all.every(
        (h) => h.db.prepare("SELECT count(*) AS n FROM relay_publication_outbox").get().n === 0,
      ),
    );
  } finally {
    release.resolve({ acknowledged: true });
    t.mock.timers.reset();
    for (const h of all) await h.close();
  }
});
