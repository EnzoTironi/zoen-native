import assert from 'node:assert/strict';
import { test } from 'node:test';
import { packageActionFixture } from './package-action.fixtures.ts';

test('real package action double tap installs once and opens exactly one canonical sheet route', async () => {
  const f = packageActionFixture();
  try {
    f.hold();
    await f.render();
    await f.press(2);
    await f.render(); // A canonical snapshot refresh must not replace the pending intent.
    assert.equal(f.effects(), 1);
    assert.equal(f.routes.length, 0);
    assert.equal(f.button()?.disabled, true);
    f.release();
    await f.settle();
    assert.equal(f.routes.length, 1);
    assert.deepEqual(JSON.parse(JSON.stringify(f.routes[0])), {
      pathname: '/miniapp/[id]',
      params: { id: 'canonical-app' },
    });
    assert.deepEqual(f.errors, []);
  } finally {
    await f.close();
  }
});

test('real package action retry and refocus resolve accepted canonical Item without another install', async () => {
  const f = packageActionFixture();
  try {
    f.loseAck();
    await f.render();
    await f.press();
    assert.equal(f.effects(), 1);
    assert.equal(f.routes.length, 0);
    await f.blur();
    await f.focus();
    await f.press();
    assert.equal(f.effects(), 1);
    assert.equal(f.routes.length, 1);
    assert.equal(f.commands.filter((c) => c.op === 'app_resource_install').length, 1);
    assert.deepEqual(f.errors, []);
  } finally {
    await f.close();
  }
});

for (const retirement of ['background', 'revokeRestore', 'changeOwner'] as const)
  test(`real package action latches ${retirement} before a late accepted Core response`, async () => {
    const f = packageActionFixture();
    try {
      f.hold();
      await f.render();
      await f.press();
      assert.equal(f.effects(), 1);
      await f[retirement]();
      if (retirement === 'background') await f.foreground();
      f.release();
      await f.settle();
      assert.equal(f.routes.length, 0);
      assert.equal(f.effects(), 1);
      assert.deepEqual(f.errors, []);
    } finally {
      await f.close();
    }
  });

test('real ordinary HTML Item offers no executable App action and makes no Core calls', async () => {
  const f = packageActionFixture('text/html');
  try {
    await f.render();
    assert.equal(f.button(), undefined);
    assert.deepEqual(f.commands, []);
  } finally {
    await f.close();
  }
});

test('real update action binds the existing App title/pin and Core scene base, then closes its chooser once', async () => {
  const f = packageActionFixture('text/html;profile=mcp-app', true);
  try {
    f.hold();
    await f.render();
    await f.press(2);
    assert.equal(f.effects(), 1);
    const command = f.commands.find((c) => c.op === 'app_resource_update');
    assert.ok(command && command.op === 'app_resource_update');
    assert.equal(command.input.admission.title, 'Existing document');
    assert.equal(command.input.item_id, 'canonical-app');
    assert.equal(
      command.input.expected_resource_uri,
      `ui://zoen/package/prior/1/${'d'.repeat(64)}`,
    );
    assert.equal(command.input.expected_version, 3);
    assert.equal(command.input.expected_sha256, 'b'.repeat(64));
    f.release();
    await f.settle();
    assert.equal(f.updated(), 1);
    assert.equal(f.routes.length, 0);
    assert.deepEqual(f.errors, []);
  } finally {
    await f.close();
  }
});
