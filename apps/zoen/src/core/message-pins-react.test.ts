import assert from 'node:assert/strict';
import { test } from 'node:test';
import { pinsReactFixture } from './message-pins-react.fixtures.ts';
import { deferred } from './store.fixtures.ts';

test('optimized Pin review sends one exact revision-bound command, then offers Unpin with current native state', async () => {
  const f = await pinsReactFixture();
  try {
    await f.renderPins();
    const click = f.buttons.get('message-pin-toggle').onPress;
    await f.act(() => {
      click();
      click();
    });
    assert.equal(f.writes.length, 1);
    assert.deepEqual(f.writes[0], {
      scope: f.nativeReview().scope,
      message_id: f.entry.id,
      change: {
        operation_id: 'operation-1',
        expected_message_revision: 'message-revision',
        expected_pin_revision: f.entry.id,
        pinned: true,
      },
    });
    assert.equal(f.buttons.get('message-pin-toggle').label, 'messagePins.unpin');
    await f.press('message-pin-toggle');
    assert.equal(f.writes.length, 2);
    assert.equal(f.writes[1].change.pinned, false);
    assert.equal(f.writes[1].change.expected_pin_revision, 'pin-revision');
    assert.deepEqual(f.errors, []);
  } finally {
    await f.close();
  }
});
for (const loss of ['background', 'blur', 'account'] as const)
  test(`retained Pin action cannot write after ${loss} or repopulate the sheet`, async () => {
    const f = await pinsReactFixture();
    try {
      await f.renderPins();
      const old = f.buttons.get('message-pin-toggle').onPress;
      if (loss === 'background') await f.act(() => f.foreground('background'));
      else if (loss === 'blur') await f.act(() => f.focus(false));
      else await f.reopen();
      await f.act(() => old());
      assert.equal(f.writes.length, 0);
      assert.equal(f.buttons.has('message-pin-toggle'), false);
      assert.deepEqual(f.errors, []);
    } finally {
      await f.close();
    }
  });

test('current group permission refusal is visible without offering an executable Pin action', async () => {
  const f = await pinsReactFixture();
  try {
    f.updateReview({ can_pin: false });
    await f.renderPins();
    assert.equal(f.buttons.has('message-pin-toggle'), false);
    assert.ok(f.text().includes('messagePins.cannotPin'));
    assert.deepEqual(f.errors, []);
  } finally {
    await f.close();
  }
});

test('bar resolves the current canonical message once before revealing and retired opens cannot navigate', async () => {
  const f = await pinsReactFixture('bar');
  const gate = deferred<unknown>();
  try {
    f.updateReview({ pinned: true });
    await f.renderPins();
    f.operation(async (command) =>
      command.op === 'message' ? gate.promise : { ...f.nativeReview() },
    );
    const open = f.buttons.get('pinned-message-open').onPress;
    await f.act(() => {
      open();
      open();
    });
    await f.act(() => gate.resolve({ ...f.entry, pinned: true }));
    assert.deepEqual(f.reveals, [f.entry.id]);
    assert.deepEqual(f.routes, []);
    await f.act(() => f.foreground('background'));
    await f.act(() => open());
    assert.deepEqual(f.reveals, [f.entry.id]);
    assert.deepEqual(f.errors, []);
  } finally {
    gate.resolve(f.entry);
    await f.close();
  }
});

test('pinned thread list opens only the destination resolved by the current core', async () => {
  const f = await pinsReactFixture('list');
  try {
    f.updateReview({ pinned: true, entry: { ...f.entry, in_thread: 'canonical-thread' } });
    await f.renderPins();
    const key = [...f.buttons.keys()].find((key) => key.startsWith('messagePins.open:'))!;
    await f.press(key);
    assert.deepEqual(f.routes, [
      {
        pathname: '/thread/[space]/[root]',
        params: { space: f.space, root: 'canonical-thread', jump: f.entry.id },
      },
    ]);
    assert.deepEqual(f.errors, []);
  } finally {
    await f.close();
  }
});

test('a native review arriving after background cannot disclose the old preview', async () => {
  const f = await pinsReactFixture();
  const gate = deferred<unknown>();
  try {
    f.operation(async () => gate.promise);
    await f.renderPins();
    await f.act(() => f.foreground('background'));
    await f.act(() => gate.resolve(f.nativeReview()));
    assert.equal(f.buttons.has('message-pin-toggle'), false);
    assert.equal(f.text().includes('Saturday meeting point'), false);
    assert.deepEqual(f.errors, []);
  } finally {
    gate.resolve(f.nativeReview());
    await f.close();
  }
});

test('a lost native write response reconciles actual shared state without resending the Pin', async () => {
  const f = await pinsReactFixture();
  try {
    f.operation(async (command) => {
      const result = await f.execute(command);
      if (command.op === 'set_message_pin') throw Error('lost native result');
      return result;
    });
    await f.renderPins();
    await f.press('message-pin-toggle');
    assert.equal(f.writes.length, 1);
    assert.equal(f.buttons.get('message-pin-toggle').label, 'messagePins.unpin');
    assert.ok(f.text().includes('messagePins.actionFailed'));
    assert.deepEqual(f.errors, []);
  } finally {
    await f.close();
  }
});

test('pinned list appends native cursor pages once and preserves accepted pages on an unchanged canonical revision', async () => {
  const f = await pinsReactFixture('list');
  let moreReads = 0;
  try {
    f.operation(async (command) => {
      if (command.op !== 'message_pins') return f.execute(command);
      if (command.cursor) {
        moreReads++;
        return {
          revision: 'stable-pins',
          entries: [
            {
              ...f.entry,
              id: 'second-pin',
              pinned: true,
              kind: { Message: { text: 'Second pinned message', card: null } },
            },
          ],
          next: null,
          total: 2,
        };
      }
      return {
        revision: 'stable-pins',
        entries: [{ ...f.entry, pinned: true }],
        next: 'native-cursor',
        total: 2,
      };
    });
    await f.renderPins();
    const more = f.buttons.get('messagePins.more').onPress;
    await f.act(() => {
      more();
      more();
    });
    assert.equal(moreReads, 1);
    assert.ok(f.text().includes('Second pinned message'));
    await f.act(() => f.canonicalChange());
    assert.ok(f.text().includes('Second pinned message'));
    assert.equal(f.buttons.has('messagePins.more'), false);
    assert.deepEqual(f.errors, []);
  } finally {
    await f.close();
  }
});

test('canonical change bursts coalesce a held native pin read into one fresh successor', async () => {
  const f = await pinsReactFixture();
  const gate = deferred<unknown>();
  let reads = 0;
  try {
    f.operation(async (command) => {
      if (command.op === 'message_pin_review' && ++reads === 1) return gate.promise;
      return f.execute(command);
    });
    await f.renderPins();
    await f.act(() => {
      f.canonicalChange();
      f.canonicalChange();
      f.canonicalChange();
    });
    assert.equal(reads, 1);
    await f.act(() => gate.resolve(f.nativeReview()));
    assert.equal(reads, 2);
    assert.equal(f.buttons.get('message-pin-toggle').disabled, false);
    assert.deepEqual(f.errors, []);
  } finally {
    gate.resolve(f.nativeReview());
    await f.close();
  }
});
