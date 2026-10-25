import assert from 'node:assert/strict';
import { test } from 'node:test';
import { packageActionFixture } from './package-action.fixtures.ts';

for (const transition of ['changeOwner', 'changeDevice', 'advanceSession'] as const)
  test(`React Compiler package action renews ${transition} with unchanged Item props`, async () => {
    const f = packageActionFixture();
    try {
      await f.render();
      await f[transition]();
      await f.press();
      const command = f.commands.find((value) => value.op === 'app_resource_find');
      assert.ok(command && command.op === 'app_resource_find');
      assert.equal(
        command.input.expected_identity_id,
        transition === 'changeOwner' ? 'new-owner' : 'owner',
      );
      assert.equal(
        command.input.expected_device_id,
        transition === 'changeDevice' ? 'new-device' : 'device',
      );
      assert.equal(f.routes.length, 1);
      assert.deepEqual(f.errors, []);
    } finally {
      await f.close();
    }
  });

test('React Compiler package action appears after opening without replacing Item props', async () => {
  const f = packageActionFixture('text/html;profile=mcp-app', false, 'opening');
  try {
    await f.render();
    assert.equal(f.button(), undefined);
    await f.phase('ready');
    assert.ok(f.button());
    await f.press();
    assert.equal(f.routes.length, 1);
    assert.deepEqual(f.errors, []);
  } finally {
    await f.close();
  }
});

test('React Compiler package action hides while locked and returns after unlock', async () => {
  const f = packageActionFixture();
  try {
    await f.render();
    assert.ok(f.button());
    await f.unlocked(false);
    assert.equal(f.button(), undefined);
    assert.deepEqual(f.commands, []);
    await f.unlocked(true);
    assert.ok(f.button());
    await f.press();
    assert.equal(f.routes.length, 1);
    assert.deepEqual(f.errors, []);
  } finally {
    await f.close();
  }
});
