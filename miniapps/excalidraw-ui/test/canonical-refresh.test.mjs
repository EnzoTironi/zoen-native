import assert from "node:assert/strict";
import test from "node:test";
import { fixture, scene, tick } from "./canonical-app-fixture.mjs";
import { sceneBytes, sceneHash } from "../src/canonical-transfer.ts";
const args = (json) => ({
  name: "refresh_scene",
  arguments: { refreshId: "new-read", version: 2, sha256: sceneHash(sceneBytes(json)) },
});

test("real canvas publishes readiness after its initial onChange and keeps read metadata", async () => {
  const f = await fixture();
  try {
    const ready = f.statuses.find((s) => s.ready);
    assert.ok(ready);
    assert.equal(ready.liveRefresh, 1);
    assert.equal(ready.itemId, "item");
    assert.equal(ready.version, 1);
    assert.equal(ready.sha256, sceneHash(sceneBytes(scene("initial"))));
    assert.equal(ready.clean, true);
  } finally {
    f.close();
  }
});

test("same canvas waits for actual application ACK, then advertises the shared head without saving it again", async () => {
  const f = await fixture();
  try {
    const next = scene("agent changed title");
    f.setReadScene(next);
    f.dom.window.qaHoldApply = true;
    const canvas = f.dom.window.document.querySelector("[data-qa-canvas]");
    let done = false;
    const update = f.app.oncalltool(args(next)).then((value) => {
      done = true;
      return value;
    });
    await f.wait(() => f.dom.window.qaScene.elements[0].text === "agent changed title");
    assert.equal(done, false);
    assert.equal(f.statuses.at(-1).version, 1);
    assert.equal(
      f.statuses.some((s) => s.version === 2),
      false,
    );
    f.dom.window.qaAcknowledge();
    const ack = await update;
    assert.deepEqual(JSON.parse(JSON.stringify(ack.structuredContent)), {
      refreshId: "new-read",
      itemId: "item",
      version: 2,
      sha256: sceneHash(sceneBytes(next)),
    });
    assert.equal(f.statuses.at(-1).version, 2);
    assert.equal(f.statuses.at(-1).clean, true);
    assert.equal(f.uploads.size, 0);
    assert.equal(f.dom.window.document.querySelector("[data-qa-canvas]"), canvas);
    assert.equal(f.edit("before host accepts"), false);
    await f.app.oncalltool({ name: "resume_scene", arguments: {} });
    await f.wait(() => f.dom.window.qaProps.viewModeEnabled === false);
    f.edit("human changed after agent");
    await f.app.onteardown({});
    assert.equal(f.uploads.size, 1);
    assert.equal(f.failures.length, 0);
  } finally {
    f.close();
  }
});

test("dirty draft and input racing the freeze ACK reject refresh without replacing canvas or discarding edits", async () => {
  for (const racing of [false, true]) {
    const f = await fixture();
    try {
      const next = scene("agent");
      f.setReadScene(next);
      if (!racing) f.edit("my draft");
      else f.dom.window.qaHoldFreeze = true;
      const update = f.app.oncalltool(args(next));
      if (racing) {
        await f.wait(() => f.dom.window.qaProps.viewModeEnabled === true);
        f.change(JSON.parse(scene("queued input")));
      }
      await assert.rejects(update, /unsaved changes/);
      assert.equal(f.dom.window.qaScene.elements[0].text, racing ? "queued input" : "my draft");
      assert.equal(
        f.statuses.some((s) => s.version === 2),
        false,
      );
      assert.equal(
        f.calls.filter((c) => c.name === "app_scene_read" && c.args.refreshId).length,
        0,
      );
    } finally {
      f.close();
    }
  }
});

test("a closed frame cannot acknowledge an outstanding canvas application", async () => {
  const f = await fixture();
  const next = scene("agent");
  f.setReadScene(next);
  f.dom.window.qaHoldApply = true;
  const update = f.app.oncalltool(args(next));
  await f.wait(() => f.dom.window.qaScene.elements[0].text === "agent");
  f.close();
  await assert.rejects(update, /closed/);
  await tick();
  assert.equal(
    f.statuses.some((s) => s.version === 2),
    false,
  );
});
