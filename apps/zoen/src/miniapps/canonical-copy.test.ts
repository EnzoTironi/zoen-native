import test from 'node:test';
import assert from 'node:assert/strict';
import { canonicalCopy } from './canonical-copy.ts';

const doc = {
  item_id: 'copy-item',
  version: 1,
  current_version: 1,
  sha256: 'a'.repeat(64),
  scene_json: '{"text":"latest café"}',
};
test('a lost copy ACK retries the exact current scene and mutation without admitting later edits', async () => {
  let prepares = 0;
  const blocked: boolean[] = [];
  const inputs: unknown[] = [];
  const copy = canonicalCopy({
    itemId: 'source',
    check: () => {},
    id: () => 'copy-mutation',
    prepare: async () => {
      prepares++;
      return { structuredContent: { uploadId: 'staged', bytes: 42, sha256: doc.sha256 } };
    },
    staged: () => ({ scene_json: doc.scene_json, mutation_id: 'guest-stage' }),
    save: async (input) => {
      inputs.push(input);
      if (inputs.length === 1) throw new Error('lost ACK');
      return doc;
    },
    blocked: (value) => blocked.push(value),
  });
  await assert.rejects(copy.run(), /lost ACK/);
  assert.equal(copy.unresolved, true);
  assert.deepEqual(blocked, [true]);
  assert.deepEqual(await copy.run(), doc);
  assert.equal(prepares, 1);
  assert.equal(inputs[0], inputs[1]);
  assert.deepEqual(inputs[0], {
    item_id: 'source',
    scene_json: doc.scene_json,
    mutation_id: 'copy-mutation',
  });
});
test('preparation failure resumes editing and a retired late copy cannot be presented', async () => {
  let finish!: () => void;
  let prepares = 0;
  const blocked: boolean[] = [];
  const copy = canonicalCopy({
    itemId: 'source',
    check: () => {},
    id: () => 'copy-mutation',
    prepare: async () => {
      if (++prepares === 1) throw new Error('capture failed');
      return { structuredContent: { uploadId: 'staged', bytes: 42, sha256: doc.sha256 } };
    },
    staged: () => ({ scene_json: doc.scene_json, mutation_id: 'guest-stage' }),
    save: async () => {
      await new Promise<void>((resolve) => {
        finish = resolve;
      });
      return doc;
    },
    blocked: (value) => blocked.push(value),
  });
  await assert.rejects(copy.run(), /capture failed/);
  assert.deepEqual(blocked, [true, false]);
  const pending = copy.run();
  await new Promise((resolve) => setImmediate(resolve));
  assert.equal(copy.run(), pending);
  copy.dispose();
  finish();
  await assert.rejects(pending, /closed/);
});
