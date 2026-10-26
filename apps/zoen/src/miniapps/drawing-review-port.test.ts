import assert from 'node:assert/strict';
import { test } from 'node:test';
import { ClientStore } from '../core/store.ts';
import { Driver, draftSession } from '../core/store.fixtures.ts';
import { drawingReply, drawingReview, sha } from '../core/drawing-review.fixtures.ts';
import { drawingReviewPort } from './drawing-review-port.ts';

async function fixture() {
  const snapshot = draftSession(),
    driver = new Driver(),
    store = new ClientStore(driver);
  const value = drawingReview(snapshot.conversations[0]!.id);
  let reads = 0,
    renders = 0,
    failRead = false,
    gate: Promise<void> | null = null;
  let change: (v: ReturnType<typeof drawingReply>) => void = () => {};
  let second: (v: typeof value) => typeof value = (v) => v;
  driver.opening = async () => snapshot;
  driver.operation = async (command) => {
    assert.equal(command.op, 'scene_tool_review');
    if (command.op !== 'scene_tool_review') throw Error('wrong op');
    assert.equal(command.input.expected_resource_uri, value.resource_uri);
    assert.equal(command.input.space_id, value.space_id);
    reads++;
    if (failRead) throw Error('Core stale/revoked');
    return reads === 2 ? second(structuredClone(value)) : structuredClone(value);
  };
  await store.open('account');
  const port = drawingReviewPort({
    store,
    pin: { itemId: value.artifact_id, resourceUri: value.resource_uri, appId: 'excalidraw' },
    spaceId: value.space_id,
    key: 'frame',
    id: () => 'nonce',
    check: () => {},
    call: async (name, args) => {
      assert.equal(name, 'render_scene_review');
      renders++;
      if (gate) await gate;
      const result = drawingReply(args);
      change(result);
      return result;
    },
  });
  const ready = () =>
    port.observe({
      zoen: { sceneSave: { itemId: value.artifact_id, ready: true, reviewPreview: 1 } },
    });
  ready();
  return {
    store,
    snapshot,
    port,
    value,
    ready,
    counts: () => ({ reads, renders }),
    mutate: (fn: typeof change) => {
      change = fn;
    },
    second: (fn: typeof second) => {
      second = fn;
    },
    fail: () => {
      failRead = true;
    },
    hold: (p: Promise<void>) => {
      gate = p;
    },
    close: async () => {
      port.dispose();
      await store.close({ force: true });
    },
  };
}

test('read-only preview uses exact Core bytes, one captured pin and a second authorization read; image provenance matches', async () => {
  const f = await fixture();
  try {
    const result = await f.port.render(f.value.request_id, f.value.content_hash);
    assert.deepEqual(result.review, f.value);
    assert.equal(
      result.before.sha256,
      sha(Buffer.from(result.before.uri.split(',')[1]!, 'base64')),
    );
    assert.notEqual(result.before.uri, result.after.uri);
    assert.deepEqual(f.counts(), { reads: 2, renders: 1 });
  } finally {
    await f.close();
  }
});
for (const field of [
  'nonce',
  'artifactId',
  'requestId',
  'contentHash',
  'resourceUri',
  'baseVersion',
  'renderer',
  'before',
  'after',
] as const)
  test(`forged renderer ${field} cannot become a reviewed picture`, async () => {
    const f = await fixture();
    try {
      f.mutate((value) => {
        (value.structuredContent as Record<string, unknown>)[field] = 'foreign';
      });
      await assert.rejects(
        f.port.render(f.value.request_id, f.value.content_hash),
        /drawing_review_/,
      );
    } finally {
      await f.close();
    }
  });

test('revocation/base changes while PNG is rendering refuse its late images', async () => {
  const f = await fixture();
  let release!: () => void;
  const pending = new Promise<void>((r) => {
    release = r;
  });
  try {
    f.hold(pending);
    const job = f.port.render(f.value.request_id, f.value.content_hash);
    await new Promise((r) => setImmediate(r));
    f.fail();
    release();
    await assert.rejects(job, /Core stale/);
  } finally {
    await f.close();
  }
});
for (const change of ['device', 'epoch'] as const)
  test(`same owner ${change} change retires an in-flight preview before returning bytes`, async () => {
    const f = await fixture();
    let release!: () => void;
    const pending = new Promise<void>((r) => {
      release = r;
    });
    try {
      f.hold(pending);
      const job = f.port.render(f.value.request_id, f.value.content_hash);
      await new Promise((r) => setImmediate(r));
      await f.store.close({ force: true });
      if (change === 'device')
        f.snapshot.account = { ...f.snapshot.account!, device_id: 'aa'.repeat(32) };
      await f.store.open('account');
      release();
      await assert.rejects(job, /owner_changed/);
    } finally {
      await f.close();
    }
  });

test('Core payload changes cannot reuse a matching request hash; harmless DTO key order is irrelevant', async () => {
  const f = await fixture();
  try {
    f.second((v) => ({ ...v, after_scene_json: '{}', after_sha256: sha('{}') }));
    await assert.rejects(f.port.render(f.value.request_id, f.value.content_hash), /changed/);
  } finally {
    await f.close();
  }
  const g = await fixture();
  try {
    g.second((v) => Object.fromEntries(Object.entries(v).reverse()) as typeof v);
    assert.ok(await g.port.render(g.value.request_id, g.value.content_hash));
  } finally {
    await g.close();
  }
});

test('old UI capability cannot invoke Core or expose previews', async () => {
  const f = await fixture();
  try {
    f.port.observe({ zoen: { sceneSave: { itemId: 'item', ready: true, liveRefresh: 1 } } });
    await assert.rejects(f.port.render(f.value.request_id, f.value.content_hash), /unavailable/);
    assert.deepEqual(f.counts(), { reads: 0, renders: 0 });
  } finally {
    await f.close();
  }
});

for (const boundary of ['bytes', 'dimensions', 'checksum'] as const)
  test(`invalid PNG ${boundary} cannot qualify a visual review`, async () => {
    const f = await fixture();
    try {
      f.mutate((reply) => {
        const image = reply.structuredContent.after;
        if (boundary === 'bytes') image.dataBase64 = 'A'.repeat(300000);
        if (boundary === 'dimensions') image.width = 897;
        if (boundary === 'checksum') image.imageSha256 = '0'.repeat(64);
      });
      await assert.rejects(f.port.render(f.value.request_id, f.value.content_hash));
      assert.deepEqual(f.counts(), { reads: 1, renders: 1 });
    } finally {
      await f.close();
    }
  });
