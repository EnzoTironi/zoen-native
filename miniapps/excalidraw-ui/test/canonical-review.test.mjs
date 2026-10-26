import assert from "node:assert/strict";
import test from "node:test";
import { fixture, tick } from "./canonical-app-fixture.mjs";
import { drawingReview } from "../../../apps/zoen/src/core/drawing-review.fixtures.ts";
const value = drawingReview();
const input = () => ({
  nonce: "preview",
  artifactId: "item",
  requestId: value.request_id,
  contentHash: value.content_hash,
  resourceUri: value.resource_uri,
  baseVersion: 1,
  beforeSceneJson: value.before_scene_json,
  beforeSha256: value.before_sha256,
  afterSceneJson: value.after_scene_json,
  afterSha256: value.after_sha256,
});
const call = (f, args = input()) =>
  f.app.oncalltool({ name: "render_scene_review", arguments: args });

test("actual SDK offscreen export renders exact before/after without touching a dirty open canvas, save/head, or its events", async () => {
  const f = await fixture();
  try {
    f.edit("private local draft");
    const before = JSON.stringify(f.dom.window.qaScene),
      calls = f.calls.length,
      state = JSON.stringify(f.statuses.at(-1));
    const reply = await call(f);
    assert.equal(reply.structuredContent.before.sceneSha256, value.before_sha256);
    assert.equal(reply.structuredContent.after.sceneSha256, value.after_sha256);
    assert.notEqual(
      reply.structuredContent.before.imageSha256,
      reply.structuredContent.after.imageSha256,
    );
    assert.deepEqual(
      JSON.parse(JSON.stringify(f.reviewExports()[0].elements)),
      JSON.parse(value.before_scene_json).elements,
    );
    assert.deepEqual(
      JSON.parse(JSON.stringify(f.reviewExports()[1].elements)),
      JSON.parse(value.after_scene_json).elements,
    );
    assert.equal(JSON.stringify(f.dom.window.qaScene), before);
    assert.equal(JSON.stringify(f.statuses.at(-1)), state);
    assert.equal(f.calls.length, calls);
    assert.equal(f.uploads.size, 0);
  } finally {
    f.close();
  }
});
for (const field of [
  "artifactId",
  "beforeSha256",
  "afterSha256",
  "contentHash",
  "nonce",
  "resourceUri",
  "baseVersion",
])
  test(`invalid review ${field} is refused without any export`, async () => {
    const f = await fixture();
    try {
      const args = input();
      args[field] = field === "baseVersion" ? 0 : field === "nonce" ? "../foreign" : "foreign";
      await assert.rejects(call(f, args), /review|checksum/);
      assert.equal(f.reviewExports().length, 0);
    } finally {
      f.close();
    }
  });

test("closing the admitted frame rejects a pending export and no late PNG is returned", async () => {
  const f = await fixture(),
    gate = f.holdReviewExport();
  const job = call(f);
  await f.wait(() => f.reviewExports().length === 1);
  f.close();
  gate.resolve();
  await assert.rejects(job, /closed/);
  await tick();
});
