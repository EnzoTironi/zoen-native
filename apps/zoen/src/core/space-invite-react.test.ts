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

test('native management paginates, renames and requires explicit revoke confirmation', async () => {
  const f = await spaceInviteFixture();
  const id = 'a'.repeat(64),
    second = 'b'.repeat(64);
  const record = {
    id,
    name: 'Family',
    creator: 'person',
    role: 'Member',
    expires_at_ms: '1800000000000',
    max_uses: 10,
    uses: 2,
    revoked: false,
  };
  f.manage(async ({ action }) => {
    if (action.kind === 'list')
      return {
        entries: [{ ...record, id: action.after ? second : id }],
        next: action.after ? '' : id,
        indexing: false,
      };
    if (action.kind === 'rename')
      return { entries: [{ ...record, name: action.name }], next: '', indexing: false };
    return {
      entries: [{ ...record, name: 'Friends', revoked: true, expires_at_ms: '0' }],
      next: '',
      indexing: false,
    };
  });
  try {
    await f.render();
    await f.press('row');
    await f.press('space-invite-manage');
    assert.equal(f.current().management?.entries[0]?.uses, 2);
    assert.deepEqual(f.requests[0], {
      op: 'manage_space_invites_reviewed',
      input: {
        space_id: f.snapshot.conversations[0]!.id,
        expected_identity_id: f.snapshot.account!.identity_id,
        expected_device_id: f.snapshot.account!.device_id,
        observed_revision: '1'.repeat(64),
      },
      action: { kind: 'list', after: '' },
    });
    await f.press(`space-invite-rename-${id}`);
    await f.act(() => f.controls.get(`space-invite-name-${id}`).onChangeText('Friends'));
    await f.press(`space-invite-save-${id}`);
    assert.equal(f.current().management?.entries[0]?.name, 'Friends');
    assert.equal(f.controls.has(`space-invite-name-${id}`), false);
    await f.press(`space-invite-revoke-${id}`);
    assert.equal(f.requests.length, 2, 'confirmation must precede mutation');
    const confirm = f.confirmations[0].find((button: any) => button.style === 'destructive');
    await f.act(() => {
      confirm.onPress();
      confirm.onPress();
    });
    assert.equal(f.requests.length, 3);
    assert.equal(f.current().management?.entries[0]?.revoked, true);
    assert.equal(f.controls.has(`space-invite-revoke-${id}`), false);
    await f.press('space-invite-next');
    assert.deepEqual(f.requests[3].action, { kind: 'list', after: id });
    assert.deepEqual(
      f.current().management?.entries.map((entry) => entry.id),
      [second],
    );
    assert.equal(f.controls.has('space-invite-next'), false);
    assert.deepEqual(f.shares, []);
    assert.deepEqual(f.errors, []);
  } finally {
    await f.close();
  }
});

test('retired management cannot disclose a page or clear the next form request', async () => {
  const f = await spaceInviteFixture();
  const old = deferred<any>(),
    fresh = deferred<any>();
  let attempts = 0;
  f.manage(() => (++attempts === 1 ? old.promise : fresh.promise));
  try {
    await f.render();
    await f.press('row');
    await f.press('space-invite-manage');
    const retired = f.current().manage;
    await f.act(() => f.current().close());
    await f.press('row');
    await f.press('space-invite-manage');
    await f.act(() => retired({ kind: 'revoke', id: 'a'.repeat(64) }));
    assert.equal(attempts, 2);
    await f.act(() => old.resolve({ entries: [], next: '', indexing: false }));
    assert.equal(f.current().management, null);
    assert.equal(f.current().busy, true);
    await f.act(() => fresh.resolve({ entries: [], next: '', indexing: true }));
    assert.equal(f.current().management?.indexing, true);
    assert.equal(f.current().busy, false);
    assert.deepEqual(f.errors, []);
  } finally {
    old.resolve({ entries: [], next: '', indexing: false });
    fresh.resolve({ entries: [], next: '', indexing: false });
    await f.close();
  }
});

test('old account revocation confirmation cannot mutate or open sharing', async () => {
  const f = await spaceInviteFixture();
  const id = 'a'.repeat(64);
  f.manage(async () => ({
    entries: [
      {
        id,
        name: 'Family',
        creator: 'person',
        role: 'Member',
        expires_at_ms: '1800000000000',
        max_uses: 1,
        uses: 0,
        revoked: false,
      },
    ],
    next: '',
    indexing: false,
  }));
  try {
    await f.render();
    await f.press('row');
    await f.press('space-invite-manage');
    await f.press(`space-invite-revoke-${id}`);
    const confirm = f.confirmations[0].find((button: any) => button.style === 'destructive');
    await f.reopen();
    await f.act(() => confirm.onPress());
    assert.equal(f.requests.length, 1);
    assert.equal(f.current().open, false);
    assert.equal(f.current().management, null);
    assert.deepEqual(f.shares, []);
    assert.deepEqual(f.errors, []);
  } finally {
    await f.close();
  }
});
