import assert from 'node:assert/strict';
import { test } from 'node:test';
import { attachmentOwnerReactFixture } from './attachment-owner-react.fixtures.ts';

test('compiled attachment composer loads after its initially closed account opens', async () => {
  const f = await attachmentOwnerReactFixture(false);
  try {
    await f.render();
    assert.equal(f.current().field, undefined);
    await f.open();
    assert.equal(f.current().field?.value, 'Owner A private caption');
    assert.deepEqual(f.errors, []);
  } finally {
    await f.close();
  }
});
for (const change of [undefined, 'identity', 'device', 'relay'] as const)
  test(`compiled attachment composer replaces retired caption: ${change ?? 'same IDs/new epoch'}`, async () => {
    const f = await attachmentOwnerReactFixture();
    try {
      await f.render();
      assert.equal(f.current().field?.value, 'Owner A private caption');
      const old = f.current().field!;
      assert.equal(f.filenames(), 'owner-a-private.jpg');
      await f.reopen(change);
      assert.equal(f.current().field?.value, 'New session private caption');
      assert.equal(f.filenames(), 'new-session-private.jpg');
      assert.equal(f.current().field?.scope.identityId, f.snapshot.account!.identity_id);
      const before = f.calls.filter((c) => c.op === 'save' || c.op === 'send').length;
      await f.act(() => old.onChange('retired caption'));
      await f.act(() => old.onSend());
      assert.equal(f.calls.filter((c) => c.op === 'save' || c.op === 'send').length, before);
      assert.deepEqual(f.errors, []);
    } finally {
      await f.close();
    }
  });
test('compiled attachment composer removes its caption on canonical conversation loss', async () => {
  const f = await attachmentOwnerReactFixture();
  try {
    await f.render();
    assert.equal(f.current().field?.value, 'Owner A private caption');
    const old = f.current().field!;
    assert.equal(f.filenames(), 'owner-a-private.jpg');
    f.snapshot.conversations = [];
    await f.act(() => f.store.refresh());
    assert.equal(f.current().field, undefined);
    assert.equal(f.current().active, false);
    assert.equal(f.filenames(), '');
    await f.act(() => old.onChange('revoked caption'));
    await f.act(() => old.onSend());
    assert.equal(f.calls.filter((c) => c.op === 'save' || c.op === 'send').length, 0);
    assert.deepEqual(f.errors, []);
  } finally {
    await f.close();
  }
});
