import assert from "node:assert/strict";
import test from "node:test";
import { fixture, completedNativeImage, pendingNativeImage, tick, jpeg } from './canonical-app-fixture.mjs';

test("actual canvas teardown freezes before latest capture, waits ACK, and only explicit resume restores editing", async () => {
  const f = await fixture();
  try {
    assert.equal(f.dom.window.qaProps.langCode, "en");
    assert.equal(f.calls.filter((c) => c.name === "app_scene_commit").length, 0);
    f.edit("latest before navigation");
    f.dom.window.qaHoldFreeze = true;
    const pending = f.app.onteardown({});
    let done = false;
    void pending.then(() => {
      done = true;
    });
    await tick();
    assert.equal(done, false);
    assert.equal(f.calls.filter((c) => c.name === "app_scene_commit").length, 0);
    await f.wait(() => f.dom.window.qaProps.viewModeEnabled);
    assert.equal(f.edit("hardware typing after freeze"), false);
    f.dom.window.qaAcknowledge();
    await pending;
    assert.equal(f.dom.window.qaProps.viewModeEnabled, true);
    assert.equal(f.calls.filter((c) => c.name === "app_scene_commit").length, 1);
    await f.app.onteardown({});
    assert.equal(f.calls.filter((c) => c.name === "app_scene_commit").length, 1);
    const saved = [...f.uploads.values()][0];
    assert.equal(
      JSON.parse(Buffer.concat(saved.chunks).toString()).elements[0].text,
      "latest before navigation",
    );
    f.hide();
    f.pointer();
    await tick();
    assert.equal(f.calls.filter((c) => c.name === "app_scene_commit").length, 1);
    await f.app.oncalltool({ name: "resume_scene" });
    await f.wait(() => !f.dom.window.qaProps.viewModeEnabled);
    assert.equal(f.edit("after resume"), true);
    assert.deepEqual(f.failures, []);
  } finally {
    f.close();
  }
});

test("actual held copy ignores hidden/pointer flushes and stays view-only without retrying old failed CAS", async () => {
  const f = await fixture();
  try {
    f.setCommitFailure(true);
    f.edit("failed A");
    await assert.rejects(f.app.onteardown({}), /shared conflict/);
    await f.wait(() => !f.dom.window.qaProps.viewModeEnabled);
    f.edit("newest B");
    const gate = f.holdAppend();
    const copy = f.app.oncalltool({ name: "prepare_scene_copy" });
    await f.wait(() => f.calls.some((c) => c.name === "app_scene_abort"));
    f.hide();
    f.pointer();
    await tick();
    assert.equal(f.calls.filter((c) => c.name === "app_scene_commit").length, 1);
    assert.equal(f.edit("late keyboard input"), false);
    gate.resolve();
    const result = await copy;
    const staged = f.uploads.get(result.structuredContent.uploadId);
    assert.equal(JSON.parse(Buffer.concat(staged.chunks).toString()).elements[0].text, "newest B");
    assert.equal(f.calls.filter((c) => c.name === "app_scene_commit").length, 1);
    assert.equal(f.dom.window.qaProps.viewModeEnabled, true);
    const before = f.calls.length;
    const replay = await f.app.oncalltool({ name: "prepare_scene_copy" });
    assert.deepEqual(replay, result);
    assert.equal(
      f.calls.length,
      before,
      "lost copy ACK retry returns exact staged result without abort/reupload",
    );
    await assert.rejects(f.app.oncalltool({ name: "resume_scene" }), /copy is already/);
    await assert.rejects(f.app.onteardown({}), /already in progress/);
    assert.equal(f.edit("still frozen"), false);
    assert.deepEqual(f.failures, []);
  } finally {
    f.close();
  }
});

test("actual failed teardown restores editing and preserves unsaved scene for exact retry", async () => {
  const f = await fixture();
  try {
    f.edit("retained");
    f.setCommitFailure(true);
    await assert.rejects(f.app.onteardown({}), /shared conflict/);
    await f.wait(() => !f.dom.window.qaProps.viewModeEnabled);
    assert.equal(f.dom.window.qaScene.elements[0].text, "retained");
    const mutation = f.calls.find((c) => c.name === "app_scene_begin").args.mutationId;
    f.setCommitFailure(false);
    await f.app.onteardown({});
    assert.equal(f.calls.filter((c) => c.name === "app_scene_begin")[1].args.mutationId, mutation);
    assert.equal(f.dom.window.qaProps.viewModeEnabled, true);
  } finally {
    f.close();
  }
});

test("actual canonical capture attaches only referenced live image bytes despite database serializer omitting files", async () => {
  const pixel = {
    id: "pixel",
    mimeType: "image/png",
    dataURL: "data:image/png;base64,iVBORw0KGgo=",
    created: 1,
  };
  const document = {
    type: "excalidraw",
    version: 2,
    elements: [{ id: "image", type: "image", fileId: "pixel", isDeleted: false }],
    appState: { viewBackgroundColor: "#fff" },
    files: { pixel, orphan: { ...pixel, id: "orphan" } },
  };
  const f = await fixture(JSON.stringify(document));
  try {
    f.dom.window.qaScene.appState.viewBackgroundColor = "#efe";
    await f.app.onteardown({});
    const saved = [...f.uploads.values()][0];
    assert.ok(saved);
    const value = JSON.parse(Buffer.concat(saved.chunks).toString());
    assert.deepEqual(value.files, { pixel });
    assert.equal(value.elements[0].fileId, "pixel");
  } finally {
    f.close();
  }
});

test("observed native pending JPEG never publishes an incomplete scene, and completion saves every element and owned byte", async () => {
  const f = await fixture();
  try {
    assert.equal(pendingNativeImage.elements.length, 6);
    assert.equal(pendingNativeImage.elements.at(-1).fileId, null);
    f.edit("older complete edit awaiting debounce");
    f.change(pendingNativeImage);
    f.pointer();
    f.hide();
    await new Promise((resolve) => setTimeout(resolve, 450));
    assert.equal(f.calls.filter((c) => c.name === "app_scene_begin").length, 0);
    assert.match(
      f.dom.window.document.querySelector('[role="status"]').textContent,
      /Importing image/,
    );
    assert.equal(f.statuses.at(-1).phase, "pending");
    f.change(completedNativeImage());
    await f.wait(() => f.calls.some((c) => c.name === "app_scene_commit"));
    await f.app.onteardown({});
    assert.equal(f.calls.filter((c) => c.name === "app_scene_commit").length, 1);
    const saved = JSON.parse(Buffer.concat([...f.uploads.values()][0].chunks).toString());
    assert.equal(saved.elements.length, 6);
    assert.equal(saved.elements.at(-1).fileId, "native_jpeg");
    assert.deepEqual(Buffer.from(saved.files.native_jpeg.dataURL.split(",")[1], "base64"), jpeg);
  } finally {
    f.close();
  }
});

for (const mode of ["teardown", "copy"]) {
  test(`actual ${mode} waits for slow native image completion before freezing and sending the whole scene`, async () => {
    const f = await fixture();
    try {
      f.change(pendingNativeImage);
      const request =
        mode === "teardown"
          ? f.app.onteardown({})
          : f.app.oncalltool({ name: "prepare_scene_copy" });
      let settled = false;
      void request.then(
        () => {
          settled = true;
        },
        () => {
          settled = true;
        },
      );
      await new Promise((resolve) => setTimeout(resolve, 25));
      assert.equal(settled, false);
      assert.equal(
        f.dom.window.qaProps.viewModeEnabled,
        false,
        "async upstream import callbacks remain enabled",
      );
      assert.equal(f.calls.filter((c) => c.name === "app_scene_begin").length, 0);
      await assert.rejects(
        mode === "copy" ? f.app.onteardown({}) : f.app.oncalltool({ name: "prepare_scene_copy" }),
        /already in progress/,
      );
      f.change(completedNativeImage());
      const reply = await request;
      assert.equal(f.dom.window.qaProps.viewModeEnabled, true);
      const upload =
        mode === "copy"
          ? f.uploads.get(reply.structuredContent.uploadId)
          : [...f.uploads.values()][0];
      const saved = JSON.parse(Buffer.concat(upload.chunks).toString());
      assert.equal(saved.elements.length, 6);
      assert.equal(
        saved.files.native_jpeg.dataURL,
        completedNativeImage().files.native_jpeg.dataURL,
      );
      assert.equal(
        f.calls.filter((c) => c.name === "app_scene_commit").length,
        mode === "copy" ? 0 : 1,
      );
    } finally {
      f.close();
    }
  });
}

test("import deadline retains editable pending scene; explicit retry after completion does not inherit an invalid mutation", async () => {
  const f = await fixture();
  try {
    f.change(pendingNativeImage);
    const teardown = f.app.onteardown({});
    await tick();
    f.expireImports();
    await assert.rejects(teardown, /still importing/);
    assert.equal(f.dom.window.qaProps.viewModeEnabled, false);
    assert.equal(f.dom.window.qaScene.elements.at(-1).fileId, null);
    assert.equal(f.calls.filter((c) => c.name === "app_scene_begin").length, 0);
    await f.wait(() =>
      /still importing/.test(f.dom.window.document.querySelector('[role="status"]').textContent),
    );
    f.change(completedNativeImage());
    await f.app.onteardown({});
    assert.equal(f.calls.filter((c) => c.name === "app_scene_commit").length, 1);
    assert.deepEqual(f.failures, []);
  } finally {
    f.close();
  }
});

test("unmount cancels an import-waiting action without a late freeze or publication", async () => {
  const f = await fixture();
  f.change(pendingNativeImage);
  const request = f.app.onteardown({});
  const rejected = assert.rejects(request, /closed/);
  await tick();
  f.close();
  await rejected;
  assert.equal(f.calls.filter((c) => c.name === "app_scene_begin").length, 0);
});

test("unmount drains an import's fire-and-forget pause behind an admitted save without an unhandled close rejection", async () => {
  const f = await fixture();
  const gate = f.holdAppend();
  f.edit("admitted complete scene");
  f.pointer();
  await f.wait(() => f.calls.some((c) => c.name === "app_scene_append"));
  f.change(pendingNativeImage);
  f.close();
  gate.resolve();
  await tick();
  await tick();
  assert.equal(f.calls.filter((c) => c.name === "app_scene_commit").length, 0);
});

test("import deadline starts while an older save is awaiting ACK; its late completion cannot freeze the editor", async () => {
  const f = await fixture();
  const gate = f.holdAppend();
  try {
    f.edit("older admitted scene");
    f.pointer();
    await f.wait(() => f.calls.some((c) => c.name === "app_scene_append"));
    f.change(pendingNativeImage);
    const teardown = f.app.onteardown({});
    await tick();
    f.expireImports();
    await assert.rejects(teardown, /still importing/);
    assert.equal(f.dom.window.qaProps.viewModeEnabled, false);
    gate.resolve();
    await f.wait(() => f.calls.some((c) => c.name === "app_scene_commit"));
    await tick();
    assert.equal(f.dom.window.qaProps.viewModeEnabled, false);
    assert.equal(f.dom.window.qaScene.elements.at(-1).fileId, null);
  } finally {
    gate.resolve();
    f.close();
  }
});

test("pending import preserves an earlier ambiguous attempt and its exact retry before the complete latest scene", async () => {
  const f = await fixture();
  try {
    f.edit("ambiguous old edit");
    f.setCommitFailure(true);
    await assert.rejects(f.app.onteardown({}), /shared conflict/);
    await f.wait(() => !f.dom.window.qaProps.viewModeEnabled);
    const failure = f.statuses.find((value) => value.phase === "error").error;
    const mutation = f.calls.find((c) => c.name === "app_scene_begin").args.mutationId;
    f.change(pendingNativeImage);
    await tick();
    assert.ok(f.dom.window.document.querySelector('[role="status"]').textContent.includes(failure));
    assert.equal(f.statuses.at(-1).phase, "error");
    const waiting = f.app.onteardown({});
    await tick();
    f.expireImports();
    await assert.rejects(waiting, /still importing/);
    await tick();
    assert.ok(f.dom.window.document.querySelector('[role="status"]').textContent.includes(failure));
    f.change(completedNativeImage());
    await tick();
    assert.equal(f.calls.filter((c) => c.name === "app_scene_begin").length, 1);
    f.setCommitFailure(false);
    await f.app.onteardown({});
    const writes = f.calls.filter((c) => c.name === "app_scene_begin");
    assert.equal(writes.length, 3);
    assert.equal(writes[1].args.mutationId, mutation);
    assert.notEqual(writes[2].args.mutationId, mutation);
    assert.equal(
      JSON.parse(Buffer.concat([...f.uploads.values()][2].chunks).toString()).elements.length,
      6,
    );
  } finally {
    f.close();
  }
});

test("image becoming incomplete during positive freeze ACK rejects without publishing or dropping it", async () => {
  const f = await fixture();
  try {
    f.edit("before import");
    f.dom.window.qaHoldFreeze = true;
    const teardown = f.app.onteardown({});
    await f.wait(() => f.dom.window.qaProps.viewModeEnabled);
    f.change(pendingNativeImage);
    await assert.rejects(teardown, /still importing/);
    await f.wait(() => !f.dom.window.qaProps.viewModeEnabled);
    assert.equal(f.calls.filter((c) => c.name === "app_scene_begin").length, 0);
    assert.equal(f.dom.window.qaScene.elements.at(-1).fileId, null);
  } finally {
    f.close();
  }
});
