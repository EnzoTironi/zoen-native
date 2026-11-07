import assert from 'node:assert/strict';
import { test } from 'node:test';
import { spaceInviteFixture } from './space-invite-react.fixtures.ts';
import { deferred } from './store.fixtures.ts';
import type { InviteDto } from './generated/InviteDto';
const link = (code: string): InviteDto => ({
  code,
  link: `https://zoen.invalid/j/${code}`,
  expires_at_ms: '1800000000000',
});

test('actual optimized invite sheet selects native limits, creates once and explicitly copies/shares', async () => {
  const f = await spaceInviteFixture();
  const response = deferred<InviteDto>();
  f.create(() => response.promise);
  try {
    await f.render();
    await f.press('row');
    assert.equal(f.controls.get('sheet').isPresented, true);
    await f.act(() => {
      f.controls.get('space-invite-expiry').onValueChange(3600);
      f.controls.get('space-invite-uses').onValueChange(1);
    });
    const create = f.controls.get('space-invite-create').onPress;
    await f.act(() => {
      create();
      create();
    });
    assert.equal(f.requests.length, 1);
    assert.deepEqual(f.requests[0], {
      op: 'create_invite_reviewed',
      input: {
        space_id: f.snapshot.conversations[0]!.id,
        expected_identity_id: f.snapshot.account!.identity_id,
        expected_device_id: f.snapshot.account!.device_id,
        observed_revision: '1'.repeat(64),
        ttl_secs: 3600,
        max_uses: 1,
      },
    });
    assert.deepEqual(f.shares, []);
    assert.deepEqual(f.copies, []);
    await f.act(() => response.resolve(link('ABCD234567')));
    assert.equal(f.current().busy, false);
    await f.press('space-invite-copy');
    assert.equal(f.controls.get('space-invite-copy').label, 'spaceInvite.copied');
    const share = f.controls.get('space-invite-share').onPress;
    await f.act(() => {
      share();
      share();
    });
    assert.deepEqual(f.copies, [link('ABCD234567').link]);
    assert.deepEqual(f.shares, f.copies);
    assert.deepEqual(f.errors, []);
  } finally {
    response.resolve(link('ABCD234567'));
    await f.close();
  }
});

test('dismissed invite creation cannot replace a later form or clear its pending state', async () => {
  const f = await spaceInviteFixture();
  const old = deferred<InviteDto>(),
    fresh = deferred<InviteDto>();
  let turn = 0;
  f.create(() => (++turn === 1 ? old.promise : fresh.promise));
  try {
    await f.render();
    await f.press('row');
    const stale = f.current().create;
    await f.act(() => stale());
    await f.act(() => f.current().close());
    await f.press('row');
    await f.act(() => f.current().create());
    await f.act(() => stale());
    assert.equal(f.requests.length, 2);
    await f.act(() => old.resolve(link('OLD1234567')));
    assert.equal(f.current().busy, true);
    assert.equal(f.current().result, null);
    await f.act(() => fresh.resolve(link('NEW1234567')));
    assert.equal(f.current().result?.code, 'NEW1234567');
    assert.equal(f.current().busy, false);
    assert.deepEqual(f.shares, []);
    assert.deepEqual(f.errors, []);
  } finally {
    old.resolve(link('OLD1234567'));
    fresh.resolve(link('NEW1234567'));
    await f.close();
  }
});

for (const interruption of ['reopen', 'background'] as const)
  test(`a pending invitation cannot reopen or disclose after ${interruption}`, async () => {
    const f = await spaceInviteFixture();
    const response = deferred<InviteDto>();
    f.create(() => response.promise);
    try {
      await f.render();
      await f.press('row');
      await f.act(() => f.current().create());
      if (interruption === 'reopen') await f.reopen();
      else await f.act(() => f.foreground('background'));
      await f.act(() => response.resolve(link('OLD1234567')));
      assert.equal(f.current().open, false);
      assert.equal(f.current().result, null);
      assert.deepEqual(f.shares, []);
      assert.deepEqual(f.errors, []);
    } finally {
      response.resolve(link('OLD1234567'));
      await f.close();
    }
  });

test('copy and share recheck native permissions before releasing the link', async () => {
  const f = await spaceInviteFixture();
  try {
    await f.render();
    await f.press('row');
    await f.act(() => f.current().create());
    f.permission(false);
    await f.press('space-invite-share');
    assert.deepEqual(f.shares, []);
    assert.equal(f.current().error, true);
    assert.deepEqual(f.errors, []);
  } finally {
    await f.close();
  }
});
