import assert from 'node:assert/strict';
import { test } from 'node:test';
import { settingsReactFixture, type SettingsKind } from './settings-route-react.fixtures.ts';

test('compiled Devices route admits Link, selection, close, refresh and explicit revoke after cold activation', async () => {
  const f = await settingsReactFixture('devices');
  try {
    await f.render();
    assert.deepEqual(f.errors, []);
    await f.act(() => f.button('device-link').onPress());
    assert.equal(f.button('authorize-device-sheet').isPresented, true);
    await f.act(() => f.button(`device-${f.other}`).onPress());
    assert.equal(f.button('device-details-sheet').isPresented, true);
    await f.act(() => f.button('deviceDirectory.close').onPress());
    assert.equal(f.button('device-details-sheet').isPresented, false);
    const before = f.calls.filter((c) => c.op === 'devices').length;
    await f.act(() => f.button('devices-refresh').onPress());
    assert.equal(f.calls.filter((c) => c.op === 'devices').length, before + 1);
    await f.act(() => f.button(`device-${f.other}`).onPress());
    await f.act(() => f.button('device-revoke').onPress());
    assert.equal(f.calls.filter((c) => c.op === 'unlink_device').length, 1);
  } finally {
    await f.close();
  }
});

test('compiled Blocked People route loads real-session profiles, opens and explicitly unblocks after cold activation', async () => {
  const f = await settingsReactFixture('blocked');
  try {
    await f.render();
    assert.deepEqual(f.errors, []);
    assert.equal(f.calls.filter((c) => c.op === 'profile').length, 1);
    await f.act(() => f.button(`blocked-person-${f.person}`).onPress());
    assert.equal(f.routes.length, 1);
    await f.act(() => f.button(`unblock-person-${f.person}`).onPress());
    assert.equal(f.calls.filter((c) => c.op === 'unblock_person').length, 1);
  } finally {
    await f.close();
  }
});

test('compiled Muted Chats route selects, opens, closes and explicitly unmutes after cold activation', async () => {
  const f = await settingsReactFixture('muted');
  try {
    await f.render();
    assert.deepEqual(f.errors, []);
    await f.act(() => f.button('muted-chat-chat-0').onPress());
    assert.equal(f.button('muted-chat-sheet').isPresented, true);
    await f.act(() => f.button('muted-chat-open').onPress());
    assert.equal(f.routes.length, 1);
    await f.act(() => f.button('common.close').onPress());
    assert.equal(f.button('muted-chat-sheet').isPresented, false);
    await f.act(() => f.button('muted-chat-chat-0').onPress());
    await f.act(() => f.button('muted-chat-unmute').onPress());
    assert.equal(f.calls.filter((c) => c.op === 'set_chat_muted').length, 1);
  } finally {
    await f.close();
  }
});

for (const kind of ['devices', 'blocked', 'muted'] as const satisfies readonly SettingsKind[]) {
  for (const transition of ['background', 'blur', 'reopen'] as const) {
    test(`compiled ${kind} fences retained callbacks across ${transition} and admits the current activation`, async () => {
      const f = await settingsReactFixture(kind);
      try {
        await f.render();
        const id =
          kind === 'devices'
            ? `device-${f.other}`
            : kind === 'blocked'
              ? `blocked-person-${f.person}`
              : 'muted-chat-chat-0';
        const old = f.button(id);
        await f.act(async () => {
          if (transition === 'background') {
            f.foreground('background');
            f.foreground('active');
          } else if (transition === 'blur') {
            f.focus(false);
            f.focus(true);
          } else await f.reopen();
          old.onPress();
        });
        assert.equal(f.routes.length, 0);
        if (kind === 'devices') assert.equal(f.button('device-details-sheet').isPresented, false);
        if (kind === 'muted') assert.equal(f.button('muted-chat-sheet').isPresented, false);
        await f.act(() => f.button(id).onPress());
        if (kind === 'blocked') assert.equal(f.routes.length, 1);
        if (kind === 'devices') assert.equal(f.button('device-details-sheet').isPresented, true);
        if (kind === 'muted') assert.equal(f.button('muted-chat-sheet').isPresented, true);
        assert.deepEqual(f.errors, []);
      } finally {
        await f.close();
      }
    });
  }
  for (const transition of ['background', 'blur', 'reopen', 'owner', 'device', 'relay'] as const) {
    test(`compiled ${kind} refuses the pending confirmation after ${transition} without redispatch`, async () => {
      const f = await settingsReactFixture(kind);
      try {
        await f.render();
        if (kind === 'devices') await f.act(() => f.button(`device-${f.other}`).onPress());
        if (kind === 'muted') await f.act(() => f.button('muted-chat-chat-0').onPress());
        let resolve!: (accepted: boolean) => void;
        const response = new Promise<boolean>((done) => {
          resolve = done;
        });
        f.confirm(() => response);
        const action =
          kind === 'devices'
            ? 'device-revoke'
            : kind === 'blocked'
              ? `unblock-person-${f.person}`
              : 'muted-chat-unmute';
        const old = f.button(action);
        await f.act(() => old.onPress());
        assert.equal(f.dialogs.length, 1);
        await f.act(async () => {
          if (transition === 'background') {
            f.foreground('background');
            f.foreground('active');
          } else if (transition === 'blur') {
            f.focus(false);
            f.focus(true);
          } else await f.reopen(transition === 'reopen' ? undefined : transition);
          resolve(true);
          old.onPress();
        });
        assert.equal(
          f.dialogs.length,
          1,
          'An old callback cannot adopt the replacement activation',
        );
        assert.deepEqual(
          f.calls.filter((c) =>
            ['unlink_device', 'unblock_person', 'set_chat_muted'].includes(c.op),
          ),
          [],
        );
        assert.deepEqual(f.errors, []);
      } finally {
        await f.close();
      }
    });
  }
}
