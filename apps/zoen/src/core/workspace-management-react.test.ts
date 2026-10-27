import assert from 'node:assert/strict';
import { test } from 'node:test';
import { workspaceManagementFixture } from './workspace-management.fixtures.ts';

test('compiled Library details enters the actual management route and opens its real release sheet', async () => {
  const f = await workspaceManagementFixture();
  try {
    await f.render();
    const details = f.controls.get('workspace-app-details:drawing');
    assert.ok(details, 'the package update cannot live behind an unreachable route');
    assert.deepEqual(
      details.ancestors,
      [],
      'details is a sibling, never inside the open-App press',
    );
    assert.match(details.label, /Shared drawing/);
    assert.equal(f.editors(), 0);
    await f.act(details.onPress);
    assert.deepEqual(f.routes, [{ pathname: '/miniapp/[id]/manage', params: { id: 'drawing' } }]);
    assert.ok(f.controls.has('app-release-choose'));
    await f.act(() => f.controls.get('app-release-choose')!.onPress());
    assert.ok(f.controls.has('package:release'));
    assert.equal(f.controls.get('package:release')!.target.id, 'drawing');
    assert.equal(f.controls.has('package:foreign'), false);
    assert.equal(f.controls.has('package:ordinary'), false);
    assert.equal(f.editors(), 0, 'management never launches or mutates the canvas');
    assert.deepEqual(f.errors, []);
  } finally {
    await f.close();
  }
});

for (const transition of ['reopen', 'revoke'] as const)
  test(`a retained Library details handler cannot navigate after ${transition}`, async () => {
    const f = await workspaceManagementFixture();
    try {
      await f.render();
      const stale = f.controls.get('workspace-app-details:drawing')!.onPress;
      await f.act(() => f[transition]());
      await f.act(stale);
      assert.deepEqual(f.routes, []);
      assert.deepEqual(f.errors, []);
    } finally {
      await f.close();
    }
  });

test('ordinary App opening is preserved and legacy resources do not gain a package-update control', async () => {
  const f = await workspaceManagementFixture(false);
  try {
    await f.render();
    assert.equal(f.controls.has('workspace-app-details:drawing'), false);
    await f.act(() => f.controls.get('workspace-item:drawing')!.onPress());
    assert.deepEqual(f.routes, [{ pathname: '/item/[id]', params: { id: 'drawing' } }]);
    assert.deepEqual(f.errors, []);
  } finally {
    await f.close();
  }
});
