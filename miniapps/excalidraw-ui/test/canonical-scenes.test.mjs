import assert from "node:assert/strict";
import test from "node:test";
import {
  canonicalSceneTransport,
  CHUNK_BYTES,
  sceneBytes,
  sceneHash,
} from "../src/canonical-transfer.ts";
import { createSceneSaves } from "../src/canonical-saves.ts";

const tick = () => new Promise((resolve) => setImmediate(resolve));
const held = () => {
  let resolve, reject;
  const promise = new Promise((ok, no) => {
    resolve = ok;
    reject = no;
  });
  return { promise, resolve, reject };
};
const scene = (text = "first") =>
  JSON.stringify({
    type: "excalidraw",
    version: 2,
    elements: [
      { id: "image", type: "image", fileId: "pixel", version: 1 },
      { id: "text", type: "text", text },
    ],
    appState: { viewBackgroundColor: "#fff", gridSize: 20 },
    files: {
      pixel: {
        id: "pixel",
        mimeType: "image/png",
        dataURL: "data:image/png;base64,iVBORw0KGgo=",
        created: 1,
      },
    },
  });
const ack = (json) => ({
  checkpointId: "item",
  version: 2,
  currentVersion: 2,
  sha256: sceneHash(sceneBytes(json)),
});
function savesFixture(save) {
  const scheduled = [],
    states = [];
  const saves = createSceneSaves({
    initial: scene(),
    save,
    publish: (value) => states.push(value),
    schedule(work) {
      let active = true;
      scheduled.push(() => {
        if (active) work();
      });
      return () => {
        active = false;
      };
    },
  });
  return {
    saves,
    states,
    run: () => {
      for (const work of scheduled.splice(0)) work();
    },
  };
}

test("full scene transport reconstructs image bytes/state and verifies ordered chunks and SHA across Unicode", async () => {
  const original = scene("🌤️".repeat(50_000));
  const bytes = sceneBytes(original);
  const upload = [],
    calls = [];
  let digest;
  const transport = canonicalSceneTransport(
    {
      async callServerTool({ name, arguments: a }) {
        calls.push({ name, a });
        let value;
        if (name === "app_scene_read")
          value = {
            transferId: "read",
            version: 1,
            currentVersion: 1,
            bytes: bytes.length,
            sha256: sceneHash(bytes),
            chunkBytes: CHUNK_BYTES,
          };
        if (name === "app_scene_read_chunk")
          value = {
            index: a.index,
            data: Buffer.from(
              bytes.subarray(a.index * CHUNK_BYTES, (a.index + 1) * CHUNK_BYTES),
            ).toString("base64"),
          };
        if (name === "app_scene_begin") {
          digest = a.sha256;
          value = { uploadId: "write", chunkBytes: CHUNK_BYTES };
        }
        if (name === "app_scene_append") {
          upload.push(Buffer.from(a.data, "base64"));
          value = { nextIndex: a.index + 1 };
        }
        if (name === "app_scene_commit") {
          assert.deepEqual(Buffer.concat(upload), Buffer.from(bytes));
          value = { checkpointId: "item", version: 2, currentVersion: 2, sha256: digest };
        }
        return { content: [], structuredContent: value };
      },
    },
    "item",
    () => {},
  );
  assert.equal(await transport.read(), original);
  assert.deepEqual(await transport.save(original, "same-attempt"), ack(original));
  assert.ok(calls.filter((c) => c.name === "app_scene_append").length > 1);
  assert.ok(calls.every((c) => JSON.stringify(c).length < 1_500_000));
  assert.equal(calls.find((c) => c.name === "app_scene_begin").a.mutationId, "same-attempt");
  assert.deepEqual(JSON.parse(original).files, JSON.parse(await transport.read()).files);
});

test("corrupt chunk and retired owner fail closed before any further read or commit", async () => {
  const bytes = sceneBytes(scene());
  let active = true,
    calls = 0;
  const transport = canonicalSceneTransport(
    {
      async callServerTool({ name }) {
        calls++;
        return {
          content: [],
          structuredContent:
            name === "app_scene_read"
              ? {
                  transferId: "read",
                  version: 1,
                  currentVersion: 1,
                  bytes: bytes.length,
                  sha256: sceneHash(bytes),
                  chunkBytes: CHUNK_BYTES,
                }
              : { index: 0, data: Buffer.from(bytes.map((n) => n ^ 1)).toString("base64") },
        };
      },
    },
    "item",
    () => {
      if (!active) throw Error("retired");
    },
  );
  await assert.rejects(transport.read(), /checksum/);
  active = false;
  const before = calls;
  await assert.rejects(transport.read(), /retired/);
  assert.equal(calls, before);
});

test("lost save ACK retains exact attempt while newer full scenes wait, and flush drains in order", async () => {
  const requests = [],
    gates = [];
  const f = savesFixture((json, mutation) => {
    requests.push({ json, mutation });
    const gate = held();
    gates.push(gate);
    return gate.promise;
  });
  const a = scene("A"),
    b = scene("B"),
    c = scene("C");
  f.saves.capture(a);
  f.run();
  await tick();
  f.saves.capture(b);
  const flush = f.saves.flush();
  let closed = false;
  void flush.then(
    () => {
      closed = true;
    },
    () => {},
  );
  assert.equal(closed, false);
  gates[0].resolve(ack(a));
  await tick();
  assert.equal(requests[1].json, b);
  gates[1].reject(Error("ACK lost"));
  await assert.rejects(flush, /ACK lost/);
  assert.equal(f.saves.getSnapshot().phase, "error");
  f.saves.capture(c);
  assert.equal(requests.length, 2);
  const retry = f.saves.flush();
  await tick();
  assert.equal(requests[2].json, b);
  assert.equal(requests[2].mutation, requests[1].mutation);
  gates[2].resolve(ack(b));
  await tick();
  assert.equal(requests[3].json, c);
  assert.notEqual(requests[3].mutation, requests[2].mutation);
  gates[3].resolve(ack(c));
  await retry;
  assert.equal(f.saves.getSnapshot().phase, "saved");
  assert.equal(f.saves.getSnapshot().sequence, 3);
  f.saves.dispose();
});

test("baseline hydration creates no public save and retirement cancels queued captures", async () => {
  let calls = 0;
  const f = savesFixture(async (json) => {
    calls++;
    return ack(json);
  });
  f.saves.baseline(scene("normalized initial"));
  await f.saves.flush();
  assert.equal(calls, 0);
  f.saves.capture(scene("unadmitted"));
  f.saves.dispose();
  f.run();
  await tick();
  assert.equal(calls, 0);
  assert.throws(() => f.saves.capture(scene("stale")), /closed/);
  await assert.rejects(f.saves.flush(), /closed/);
});

test("accepted old receipt cannot label a newer shared version as saved", async () => {
  const bytes = sceneBytes(scene());
  const transport = canonicalSceneTransport(
    {
      async callServerTool({ name, arguments: a }) {
        return {
          content: [],
          structuredContent:
            name === "app_scene_begin"
              ? { uploadId: "write", chunkBytes: CHUNK_BYTES }
              : name === "app_scene_append"
                ? { nextIndex: a.index + 1 }
                : { checkpointId: "item", version: 2, currentVersion: 3, sha256: sceneHash(bytes) },
        };
      },
    },
    "item",
    () => {},
  );
  await assert.rejects(transport.save(scene(), "same-intent"), /newer version/);
});

test("pause drains admitted save without retrying or sending a newer scene; copy stages latest bytes without commit", async () => {
  const gate = held(),
    requests = [];
  const f = savesFixture((json, mutation) => {
    requests.push({ json, mutation });
    return gate.promise;
  });
  f.saves.capture(scene("A"));
  f.run();
  await tick();
  f.saves.capture(scene("B"));
  const pause = f.saves.pause();
  gate.reject(Error("old conflict"));
  await pause;
  assert.equal(requests.length, 1);
  f.run();
  await tick();
  assert.equal(requests.length, 1);
  const calls = [],
    uploads = new Map();
  let fail = true;
  const transport = canonicalSceneTransport(
    {
      async callServerTool({ name, arguments: a }) {
        calls.push({ name, a });
        let value;
        if (name === "app_scene_begin") {
          const uploadId = "u" + uploads.size;
          uploads.set(uploadId, { ...a, chunks: [] });
          value = { uploadId, chunkBytes: CHUNK_BYTES };
        }
        if (name === "app_scene_append") {
          uploads.get(a.uploadId).chunks[a.index] = Buffer.from(a.data, "base64");
          value = { nextIndex: a.index + 1 };
        }
        if (name === "app_scene_commit") {
          if (fail) throw Error("shared conflict");
          value = ack(scene("A"));
        }
        if (name === "app_scene_abort") value = {};
        return { content: [], structuredContent: value };
      },
    },
    "item",
    () => {},
  );
  await assert.rejects(transport.save(scene("A"), "original-mutation"), /conflict/);
  const staged = await transport.prepareCopy(scene("B"));
  assert.equal(calls.filter((c) => c.name === "app_scene_commit").length, 1);
  assert.equal(calls.filter((c) => c.name === "app_scene_abort").length, 1);
  assert.equal(Buffer.concat(uploads.get(staged.uploadId).chunks).toString(), scene("B"));
  const mutation = uploads.get(staged.uploadId).mutationId;
  const repeated = await transport.prepareCopy(scene("B"));
  assert.equal(uploads.get(repeated.uploadId).mutationId, mutation);
  f.saves.dispose();
});
