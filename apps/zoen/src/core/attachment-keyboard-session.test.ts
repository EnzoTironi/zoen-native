import assert from 'node:assert/strict';
import { test } from 'node:test';
import { AttachmentKeyboardSession } from '../media/attachment-keyboard-session.ts';
const deferred = () => {
  let resolve!: () => void, reject!: (error: Error) => void;
  const promise = new Promise<void>((yes, no) => {
    resolve = yes;
    reject = no;
  });
  return { promise, resolve, reject };
};
const tick = async () => {
  for (let i = 0; i < 8; i++) await Promise.resolve();
};
function fixture() {
  let next = 0;
  const session = new AttachmentKeyboardSession('space:thread', true, () => `token-${++next}`);
  const unbind = session.bind({
    tag: 7,
    context: 'space:normal',
    label: 'Message',
    placeholder: 'Message',
  });
  const done = deferred();
  const calls: string[] = [];
  const unregister = session.register(async (kind) => {
    calls.push(kind);
    if (kind === 'photo') await done.promise;
  }, true);
  const open = () => {
    session.openAttachments();
    const id = session.getSnapshot().presentation!.id;
    session.admitPicker(id);
    return id;
  };
  return { session, done, calls, open, unbind, unregister };
}
test('explicit plus requires exact mounted field and enabled route, then admits one Photos owner', () => {
  let next = 0;
  const session = new AttachmentKeyboardSession('s', true, () => `${++next}`);
  let calls = 0;
  session.register(async () => {
    calls++;
  }, true);
  session.openAttachments();
  assert.equal(calls, 0);
  session.bind({ tag: 3, context: 'c', label: 'Message', placeholder: '' });
  session.configure('s', false);
  session.openAttachments();
  assert.equal(calls, 0);
  session.configure('s', true);
  session.openAttachments();
  assert.equal(calls, 1);
  session.openAttachments();
  assert.equal(calls, 1);
  assert.equal(session.getSnapshot().presentation?.closing, true);
});
test('Files waits real native removal AND completion of current Photos picker, in either order', async () => {
  for (const nativeFirst of [true, false]) {
    const f = fixture(),
      id = f.open();
    f.session.handoff(id, 'document');
    if (nativeFirst) f.session.closed(id);
    else {
      f.done.resolve();
      await tick();
    }
    assert.deepEqual(f.calls, ['photo']);
    if (nativeFirst) f.done.resolve();
    else f.session.closed(id);
    await tick();
    assert.deepEqual(f.calls, ['photo', 'document']);
    assert.equal(f.session.getSnapshot().presentation, null);
    f.session.closed(id);
    await tick();
    assert.deepEqual(f.calls, ['photo', 'document']);
  }
});
test('retiring field same tag with a new context rejects queued selection, close and Files handoff', async () => {
  const f = fixture(),
    old = f.open();
  f.session.handoff(old, 'document');
  f.session.bind({ tag: 7, context: 'space:edit', label: 'Edit', placeholder: '' });
  f.session.toggleEmoji();
  const fresh = f.session.getSnapshot().presentation!;
  assert.notEqual(fresh.id, old);
  assert.equal(f.session.closed(old), false);
  f.done.resolve();
  await tick();
  assert.deepEqual(f.calls, ['photo']);
  assert.equal(f.session.getSnapshot().presentation, fresh);
});
test('old picker rejection cannot retire a newer emoji presentation', async () => {
  const f = fixture();
  f.open();
  f.session.retire();
  f.session.toggleEmoji();
  const fresh = f.session.getSnapshot().presentation;
  f.done.reject(new Error('old picker'));
  await tick();
  assert.equal(f.session.getSnapshot().presentation, fresh);
});
test('old field cleanup cannot unregister replacement editor or close its panel', () => {
  const f = fixture();
  f.session.bind({ tag: 8, context: 'space:normal', label: 'Message', placeholder: '' });
  f.session.toggleEmoji();
  const fresh = f.session.getSnapshot().presentation;
  f.unbind();
  assert.equal(f.session.getSnapshot().field?.tag, 8);
  assert.equal(f.session.getSnapshot().presentation, fresh);
});
test('search disable then resume cannot revive a retired handoff token', async () => {
  const f = fixture(),
    id = f.open();
  f.session.handoff(id, 'audio');
  f.session.configure('space:thread', false);
  f.session.configure('space:thread', true);
  f.session.closed(id);
  f.done.resolve();
  await tick();
  assert.deepEqual(f.calls, ['photo']);
  f.session.toggleEmoji();
  assert.notEqual(f.session.getSnapshot().presentation?.id, id);
});
test('background and route blur retire pending native callbacks before renewed focus', async () => {
  for (const retire of [
    (s: AttachmentKeyboardSession) => {
      s.setForeground(false);
      s.setForeground(true);
    },
    (s: AttachmentKeyboardSession) => {
      s.setFocused(false);
      s.setFocused(true);
    },
  ]) {
    const f = fixture(),
      id = f.open();
    f.session.handoff(id, 'document');
    retire(f.session);
    f.session.closed(id);
    f.done.resolve();
    await tick();
    assert.deepEqual(f.calls, ['photo']);
    assert.equal(f.session.getSnapshot().presentation, null);
  }
});
test('permission alert inactivity may settle the existing close but cannot dispatch another category while inactive', async () => {
  const f = fixture(),
    id = f.open();
  f.session.handoff(id, 'audio');
  f.session.setForeground(false, false);
  assert.equal(f.session.owns(id), true);
  assert.equal(f.session.isCurrent(id), false);
  assert.equal(f.session.closed(id), true);
  f.done.resolve();
  await tick();
  assert.deepEqual(f.calls, ['photo']);
  assert.equal(f.session.getSnapshot().presentation, null);
});
test('Audio is absent without real capability and revoked capability blocks an already queued handoff', async () => {
  const f = fixture(),
    id = f.open();
  f.session.handoff(id, 'audio');
  f.session.register(async (kind) => {
    f.calls.push(kind);
  }, false);
  f.session.closed(id);
  f.done.resolve();
  await tick();
  assert.deepEqual(f.calls, ['photo']);
  const next = f.open();
  f.session.handoff(next, 'audio');
  assert.equal(f.session.getSnapshot().presentation?.closing, false);
});
test('emoji switch keeps same field, waits Photos ownership and never calls media pick again', async () => {
  const f = fixture(),
    id = f.open(),
    field = f.session.getSnapshot().field;
  f.session.toggleEmoji();
  assert.equal(f.session.getSnapshot().presentation?.returnToKeyboard, true);
  f.session.closed(id);
  assert.equal(f.session.getSnapshot().presentation?.kind, 'attachments');
  f.done.resolve();
  await tick();
  assert.deepEqual(f.calls, ['photo']);
  assert.equal(f.session.getSnapshot().presentation?.kind, 'emoji');
  assert.equal(f.session.getSnapshot().field, field);
});
test('explicit keyboard return clears one emoji owner only after its own native close', () => {
  const f = fixture();
  f.session.toggleEmoji();
  const id = f.session.getSnapshot().presentation!.id;
  f.session.closeKeyboardPanel();
  assert.equal(f.session.getSnapshot().presentation?.closing, true);
  assert.equal(f.session.closed('older'), false);
  assert.notEqual(f.session.getSnapshot().presentation, null);
  f.session.closed(id);
  assert.equal(f.session.getSnapshot().presentation, null);
  assert.deepEqual(f.calls, []);
});
test('unregistered callbacks cannot receive a pending Files action after native close', async () => {
  const f = fixture(),
    id = f.open();
  f.session.handoff(id, 'document');
  f.unregister();
  f.session.closed(id);
  f.done.resolve();
  await tick();
  assert.deepEqual(f.calls, ['photo']);
});

test('a rejected Photos admission has no native pane to await and cannot strand the plus state', async () => {
  const f = fixture();
  f.session.register(async () => {}, false);
  f.session.openAttachments();
  await tick();
  assert.equal(f.session.getSnapshot().presentation, null);
});
test('category waits the newly committed enabled callback and fences replacement scope before queued dispatch', async () => {
  const f = fixture(),
    id = f.open();
  f.session.register(
    async (kind) => {
      f.calls.push(kind);
    },
    true,
    false,
  );
  f.session.handoff(id, 'document');
  f.session.closed(id);
  f.done.resolve();
  await tick();
  assert.deepEqual(f.calls, ['photo']);
  f.session.register(
    async (kind) => {
      f.calls.push(kind);
    },
    true,
    true,
  );
  assert.deepEqual(f.calls, ['photo']);
  f.session.configure('replacement-space', true);
  await tick();
  assert.deepEqual(f.calls, ['photo']);
});
