import assert from "node:assert/strict";
import test from "node:test";
import { build } from "esbuild";
import { fileURLToPath } from "node:url";
import { fixture, scene, held } from "./canonical-app-fixture.mjs";
import { sceneBytes, sceneHash } from "../src/canonical-transfer.ts";

const native = await build({
  stdin: {
    contents: `export{CanonicalSceneHost}from'./canonical-scene.ts';export{canonicalRefresh}from'./canonical-refresh.ts';`,
    resolveDir: fileURLToPath(new URL("../../../apps/zoen/src/miniapps", import.meta.url)),
    loader: "ts",
  },
  bundle: true,
  write: false,
  platform: "node",
  format: "esm",
});
const { CanonicalSceneHost, canonicalRefresh } = await import(
  "data:text/javascript;base64," +
    Buffer.from(
      native.outputFiles[0].text + "\n//# sourceURL=zoen-native-refresh-fixture.mjs",
    ).toString("base64")
);
const document = (version, json) => ({
  item_id: "item",
  version,
  current_version: version,
  scene_json: json,
  sha256: sceneHash(sceneBytes(json)),
});
async function composed() {
  const f = await fixture();
  let active = true,
    admitted = true,
    reads = 0,
    conflicts = 0;
  let next = document(2, scene("approved agent title"));
  const writes = [];
  let toolAckGate = null,
    loseResumeAck = false,
    loseRefreshAck = false;
  const check = () => {
    if (!active) throw Error("owner changed");
  };
  const host = new CanonicalSceneHost({
    initialDoc: document(1, scene("initial")),
    check,
    id: () => "scene",
    save: async (input) => {
      check();
      writes.push(input);
      return document(3, input.scene_json);
    },
  });
  f.setNativeCall((name, args) => host.call(name, args));
  const refresh = canonicalRefresh({
    itemId: "item",
    scene: host,
    check,
    read: async () => {
      reads++;
      check();
      if (!admitted) throw Error("resource pin changed");
      return next;
    },
    call: async (name, args) => {
      const result = await f.app.oncalltool({ name, arguments: args });
      if (name === "refresh_scene" && toolAckGate) await toolAckGate.promise;
      if (name === "refresh_scene" && loseRefreshAck) {
        loseRefreshAck = false;
        throw Error("lost refresh tool ACK");
      }
      if (name === "resume_scene" && loseResumeAck) {
        loseResumeAck = false;
        throw Error("lost resume ACK");
      }
      return result;
    },
    conflict: () => {
      conflicts++;
    },
  });
  const publish = f.app.updateModelContext;
  f.app.updateModelContext = async (value) => {
    refresh.observe(value.structuredContent);
    return publish(value);
  };
  refresh.observe({ zoen: { sceneSave: f.statuses.at(-1) } });
  return {
    ...f,
    holdToolAck: () => {
      toolAckGate = held();
      return toolAckGate;
    },
    loseRefreshAck: () => {
      loseRefreshAck = true;
    },
    loseResumeAck: () => {
      loseResumeAck = true;
    },
    host,
    refresh,
    writes,
    revoke: () => {
      admitted = false;
    },
    stop: () => {
      active = false;
    },
    state: () => ({ reads, conflicts }),
    setNext: (doc) => {
      next = doc;
    },
    close() {
      refresh.dispose();
      host.dispose();
      f.close();
    },
  };
}

test("actual SDK canvas + host: approved shared head applies in the same session only after onChange, then next edit uses that CAS", async () => {
  const f = await composed();
  try {
    const canvas = f.dom.window.document.querySelector("[data-qa-canvas]");
    f.dom.window.qaHoldApply = true;
    const update = f.refresh.refresh();
    await f.wait(() => f.dom.window.qaScene.elements[0].text === "approved agent title");
    assert.equal(f.host.head().version, 1, "reading candidate is not authority to save against it");
    assert.equal(f.refresh.readiness.getSnapshot(), false);
    f.dom.window.qaAcknowledge();
    await update;
    assert.equal(f.host.head().version, 2);
    assert.equal((await f.refresh.observedHead()).version, 2);
    assert.equal(f.writes.length, 0);
    assert.equal(f.dom.window.document.querySelector("[data-qa-canvas]"), canvas);
    await f.wait(() => f.dom.window.qaProps.viewModeEnabled === false);
    f.edit("my followup");
    await f.app.onteardown({});
    assert.equal(f.writes.length, 1);
    assert.equal(f.writes[0].expected_version, 2);
    assert.equal(f.writes[0].expected_sha256, document(2, scene("approved agent title")).sha256);
    assert.equal(JSON.parse(f.writes[0].scene_json).elements[0].text, "my followup");
    assert.equal(f.host.head().version, 3);
    assert.deepEqual(f.failures, []);
  } finally {
    f.close();
  }
});

test("actual SDK dirty input refuses shared refresh and keeps both the canvas draft and the prior host CAS", async () => {
  const f = await composed();
  try {
    f.edit("do not overwrite my draft");
    await f.refresh.refresh();
    assert.equal(f.host.head().version, 1);
    assert.equal(f.dom.window.qaScene.elements[0].text, "do not overwrite my draft");
    assert.equal(f.state().conflicts, 1);
    assert.equal(f.writes.length, 0);
    assert.equal(f.refresh.readiness.getSnapshot(), false);
  } finally {
    f.close();
  }
});

test("actual pinned read refusal never asks the canvas to adopt another release", async () => {
  const f = await composed();
  try {
    f.revoke();
    await assert.rejects(f.refresh.refresh(), /resource pin changed/);
    assert.equal(f.dom.window.qaScene.elements[0].text, "initial");
    assert.equal(f.host.head().version, 1);
    assert.equal(f.writes.length, 0);
    assert.equal(
      f.calls.some((c) => c.args.refreshId),
      false,
    );
  } finally {
    f.close();
  }
});

test("owner change before delayed canvas ACK retires the pending candidate and emits no later accepted head", async () => {
  const f = await composed();
  f.dom.window.qaHoldApply = true;
  const update = f.refresh.refresh();
  await f.wait(() => f.dom.window.qaScene.elements[0].text === "approved agent title");
  f.stop();
  f.dom.window.qaAcknowledge();
  await assert.rejects(update, /owner changed|closed/);
  assert.equal(f.writes.length, 0);
  assert.equal(f.refresh.readiness.getSnapshot(), false);
  f.close();
});

test("a queued second remote change is applied after v2 ACK and reaches v3 without a remount or extra save", async () => {
  const f = await composed();
  try {
    f.dom.window.qaHoldApply = true;
    const first = f.refresh.refresh();
    await f.wait(() => f.dom.window.qaScene.elements[0].text === "approved agent title");
    f.setNext(document(3, scene("second approved change")));
    const queued = f.refresh.refresh();
    assert.equal(queued, first, "one retained refresh task owns the queue");
    f.dom.window.qaAcknowledge();
    await f.wait(() => f.dom.window.qaScene.elements[0].text === "second approved change");
    assert.equal(f.host.head().version, 2);
    f.dom.window.qaAcknowledge();
    await first;
    assert.equal(f.host.head().version, 3);
    assert.equal((await f.refresh.observedHead()).version, 3);
    assert.equal(f.writes.length, 0);
    assert.equal(f.state().reads, 2);
  } finally {
    f.close();
  }
});

test("missing application ACK times out without advancing CAS and preserves a copyable editable canvas", async () => {
  const f = await composed();
  try {
    f.dom.window.qaHoldApply = true;
    const update = f.refresh.refresh();
    await f.wait(() => f.dom.window.qaScene.elements[0].text === "approved agent title");
    f.expireImports(); // The fixture runs only the real 10s deadlines, without sleeping.
    await assert.rejects(update, /did not acknowledge/);
    assert.equal(f.host.head().version, 1);
    assert.equal(f.writes.length, 0);
    await f.wait(() => f.dom.window.qaProps.viewModeEnabled === false);
    assert.equal(f.edit("recovered local draft"), true);
    const copy = await f.app.oncalltool({ name: "prepare_scene_copy", arguments: {} });
    assert.ok(copy.structuredContent.uploadId);
    const staged = f.host.stagedScene(
      copy.structuredContent.uploadId,
      copy.structuredContent.bytes,
      copy.structuredContent.sha256,
    );
    assert.equal(JSON.parse(staged.scene_json).elements[0].text, "recovered local draft");
    assert.equal(f.host.head().version, 1);
    assert.equal(f.writes.length, 0);
  } finally {
    f.close();
  }
});

test("lost refresh context ACK can be retried in the same frame without accepting an unacknowledged save base", async () => {
  const f = await composed();
  try {
    const publish = f.app.updateModelContext;
    let fail = true;
    f.app.updateModelContext = async (value) => {
      if (value.structuredContent.zoen.sceneSave.version === 2 && fail) {
        fail = false;
        throw Error("lost context ACK");
      }
      return publish(value);
    };
    await assert.rejects(f.refresh.refresh(), /lost context ACK/);
    assert.equal(f.host.head().version, 1);
    await f.wait(
      () => f.dom.window.qaProps.viewModeEnabled === false && f.statuses.at(-1).version === 2,
    );
    assert.equal(f.refresh.readiness.getSnapshot(), false);
    await f.refresh.refresh();
    assert.equal(f.host.head().version, 2);
    assert.equal((await f.refresh.observedHead()).version, 2);
    assert.equal(f.writes.length, 0);
    assert.equal(f.dom.window.qaScene.elements[0].text, "approved agent title");
  } finally {
    f.close();
  }
});

test("canvas stays readonly while the host is waiting for its refresh ACK, preventing a new edit on the old save base", async () => {
  const f = await composed();
  try {
    const gate = f.holdToolAck();
    const update = f.refresh.refresh();
    await f.wait(() => f.statuses.at(-1).version === 2);
    assert.equal(f.host.head().version, 1);
    assert.equal(f.edit("must not race the host"), false);
    assert.equal(f.writes.length, 0);
    gate.resolve();
    await update;
    assert.equal(f.host.head().version, 2);
    await f.wait(() => f.dom.window.qaProps.viewModeEnabled === false);
    assert.equal(f.edit("after the host accepted"), true);
  } finally {
    f.close();
  }
});

test("lost resume ACK retries the same already-applied head without another refresh or save", async () => {
  const f = await composed();
  try {
    f.loseResumeAck();
    await assert.rejects(f.refresh.refresh(), /lost resume ACK/);
    assert.equal(f.host.head().version, 2, "canvas ACK was already accepted before resuming");
    assert.equal(f.refresh.readiness.getSnapshot(), false);
    await f.refresh.refresh();
    assert.equal((await f.refresh.observedHead()).version, 2);
    assert.equal(f.writes.length, 0);
    assert.equal(f.calls.filter((c) => c.args.refreshId).length, 1, "candidate was applied once");
  } finally {
    f.close();
  }
});

test("a lost successful refresh tool response keeps readonly and one explicit retry requalifies the same canvas", async () => {
  const f = await composed();
  try {
    f.loseRefreshAck();
    await assert.rejects(f.refresh.refresh(), /lost refresh tool ACK/);
    assert.equal(f.host.head().version, 1);
    assert.equal(f.edit("still awaiting the host"), false);
    assert.equal(f.refresh.readiness.getSnapshot(), false);
    await f.refresh.refresh();
    assert.equal((await f.refresh.observedHead()).version, 2);
    await f.wait(() => f.dom.window.qaProps.viewModeEnabled === false);
    assert.equal(f.dom.window.qaScene.elements[0].text, "approved agent title");
    assert.equal(f.writes.length, 0);
  } finally {
    f.close();
  }
});
