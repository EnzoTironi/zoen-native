// Unit scheduling/failure tests with actual SQLite and Rust/WASM wire codecs.
// Host/session hooks below are unit doubles, not native/Directory authentication
// acceptance. The separate real dual-wire journey proves the authenticated path.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { DatabaseSync } from "node:sqlite";
import test from "node:test";
import { RelayComposition } from "../src/relay-composition.mjs";
import {
  initSync,
  relaySocketFrame,
  relaySocketControl,
  relaySocketRequestEffects,
} from "../generated/wasm/admission.js";

initSync({
  module: await readFile(new URL("../generated/wasm/admission_bg.wasm", import.meta.url)),
});
const native = JSON.parse(
  await readFile(
    process.env.ZOEN_DIRECTORY_FIXTURE ??
      new URL("../generated/directory-fixture.json", import.meta.url),
    "utf8",
  ),
);
const step = native.sealed_profiles.steps.find(
  (step) => step.encrypted_test_case === "profile-first",
);
assert.ok(step, "Real signed native PutProfile input is required");
const refusal = native.sealed_profiles.steps.find(
  (step) => step.encrypted_test_case === "profile-same-version-different",
);
assert.ok(refusal?.refused_reply_base64);
const frame = Uint8Array.from(Buffer.from(step.frame_base64, "base64"));
const input = JSON.parse(relaySocketFrame(frame));
const identity = input.profile_changed.identity;
const peer = "ab".repeat(32),
  removed = "cd".repeat(32),
  stranger = "ef".repeat(32);
const id = (n) => n.toString(16).padStart(64, "0");

function harness({ failedReply = false, failureAt = null, additional = [] } = {}) {
  const db = new DatabaseSync(":memory:");
  const pending = [],
    sent = [],
    reads = [],
    order = [];
  const sockets = [identity, identity, peer, removed, stranger].map((identity, n) => ({
    readyState: 1,
    row: { id: id(n + 1), stage: "ready", identity },
  }));
  const rows = (query, ...args) => {
    const result = db.prepare(query).all(...args);
    return Object.assign(result, { toArray: () => Array.from(result) });
  };
  const ctx = {
    storage: {
      sql: { exec: rows },
      transactionSync(work) {
        db.exec("BEGIN");
        try {
          const result = work();
          assert.equal(result instanceof Promise, false);
          db.exec("COMMIT");
          return result;
        } catch (error) {
          db.exec("ROLLBACK");
          throw error;
        }
      },
    },
    getWebSockets: () => sockets,
    waitUntil: (promise) => pending.push(promise),
  };
  const reply = Uint8Array.from(
    Buffer.from(failedReply ? refusal.refused_reply_base64 : step.expected.reply_base64, "base64"),
  );
  const host = {
    socketId: (socket) => socket.row.id,
    socketFor: (connection) =>
      sockets.find((socket) => socket.row.id === connection && socket.readyState === 1),
    socketRow: (socket) => socket.row,
    incoming: (bytes) => JSON.parse(relaySocketFrame(bytes)),
    admit: () => ({
      state: "admitted",
      identity,
      registered: true,
      origin_connection: id(1),
      session: { authenticated_identity: identity, authenticated_device: "01".repeat(32) },
    }),
    request: () => {
      order.push("store");
      return reply;
    },
    effects: (bytes) => JSON.parse(relaySocketRequestEffects(bytes)),
    control: (value) =>
      Uint8Array.from(
        Buffer.from(JSON.parse(relaySocketControl(JSON.stringify(value))).frame_base64, "base64"),
      ),
    ownStock: () => assert.fail("PutProfile is not a key-package publication"),
    claimedStock: () => assert.fail("PutProfile is not a key-package claim"),
    send: (socket, bytes) => {
      sent.push({ connection: socket.row.id, bytes: Buffer.from(bytes) });
      order.push("send");
      return true;
    },
    close: () => assert.fail("A best-effort profile notice must not close the requester"),
    wake: async () => {},
    reason: (error) => ({ reason: error.message }),
  };
  const spaces = {
    async coMembers({ spaceId, session }) {
      order.push("co-members");
      reads.push(spaceId);
      assert.equal(session.authenticated_identity, identity);
      if (spaceId === failureAt) throw new Error("Unit co-members dependency failure");
      return { members: spaceId.endsWith("removed") ? [] : [peer, identity, ...additional] };
    },
  };
  const control = new RelayComposition(ctx, spaces, host);
  for (const space of ["sp_profile_a", "sp_profile_b", "sp_profile_removed"])
    db.prepare("INSERT INTO relay_discovery VALUES (?,?)").run(identity, space);
  return {
    sent,
    reads,
    order,
    reply: Buffer.from(reply),
    async run() {
      control.enqueue(
        sockets[0],
        failedReply ? Uint8Array.from(Buffer.from(refusal.frame_base64, "base64")) : frame,
      );
      for (let index = 0; index < pending.length; index++) await pending[index];
      assert.equal(db.prepare("SELECT count(*) AS n FROM relay_pending").get().n, 0);
    },
    close: () => db.close(),
  };
}

test("successful profile notice excludes exact origin but reaches linked device and deduplicated current peer before Res", async () => {
  const h = harness();
  try {
    await h.run();
    assert.deepEqual(h.reads.sort(), ["sp_profile_a", "sp_profile_b", "sp_profile_removed"]);
    assert.deepEqual(
      h.sent.map((sent) => sent.connection),
      [id(2), id(3), id(1)],
    );
    const expected = Buffer.from(
      JSON.parse(
        relaySocketControl(
          JSON.stringify({
            kind: "profile_changed",
            ...input.profile_changed,
          }),
        ),
      ).frame_base64,
      "base64",
    );
    assert.deepEqual(h.sent[0].bytes, expected);
    assert.deepEqual(h.sent[1].bytes, expected);
    assert.deepEqual(h.sent[2].bytes, h.reply);
    assert.equal(h.order[0], "store");
    assert.ok(h.order.lastIndexOf("co-members") < h.order.indexOf("send"));
  } finally {
    h.close();
  }
});

test("any discovery dependency failure suppresses every partial notice and keeps the successful original Res", async () => {
  const h = harness({ failureAt: "sp_profile_b" });
  try {
    await h.run();
    assert.deepEqual(h.sent, [{ connection: id(1), bytes: h.reply }]);
  } finally {
    h.close();
  }
});

test("refused PutProfile never consults Space or broadcasts caller-derived metadata", async () => {
  const h = harness({ failedReply: true });
  try {
    await h.run();
    assert.deepEqual(h.reads, []);
    assert.deepEqual(h.sent, [{ connection: id(1), bytes: h.reply }]);
  } finally {
    h.close();
  }
});

test("invalid or oversized co-member sets suppress the whole notice set without changing committed reply", async () => {
  for (const additional of [["invalid-identity"], Array.from({ length: 1001 }, () => peer)]) {
    const h = harness({ additional });
    try {
      await h.run();
      assert.deepEqual(h.sent, [{ connection: id(1), bytes: h.reply }]);
    } finally {
      h.close();
    }
  }
});
