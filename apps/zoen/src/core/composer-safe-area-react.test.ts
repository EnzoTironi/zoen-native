import assert from 'node:assert/strict';
import { test } from 'node:test';
import { composerSafeAreaFixture, type Frame } from './composer-safe-area-react.fixtures.ts';
const rotations: Frame[] = [
  { width: 402, height: 874, left: 0, right: 0, top: 62, bottom: 34 },
  { width: 874, height: 402, left: 62, right: 0, top: 0, bottom: 21 },
  { width: 874, height: 402, left: 0, right: 62, top: 0, bottom: 21 },
  { width: 874, height: 402, left: 62, right: 62, top: 0, bottom: 21 },
  { width: 402, height: 874, left: 0, right: 0, top: 62, bottom: 34 },
];
for (const searching of [true, false])
  test(`compiled ${searching ? 'search close/navigation' : 'composer attachment/send/reply'} follows current safe edges while retaining focused input`, async () => {
    const f = await composerSafeAreaFixture(searching);
    try {
      await f.render();
      f.focus();
      await f.act(() => f.control('message-input').onChangeText('Retained focused text'));
      for (const frame of rotations) {
        await f.rotate(frame);
        const safe = f.style('message-composer-safe-area');
        assert.equal(safe.paddingLeft, frame.left + 8);
        assert.equal(safe.paddingRight, frame.right + 8);
        assert.equal(safe.paddingTop, 0);
        assert.equal(safe.paddingBottom, Math.max(frame.bottom, 12));
        const leading = f.style(searching ? 'close-conversation-search' : 'add-attachment');
        assert.equal(leading.width, 40);
        assert.ok(
          safe.paddingLeft - 2 >= frame.left,
          'including hitSlop, leading control is outside the hardware cutout',
        );
        const trailing = f.style(searching ? 'newer-search-result' : 'send-message');
        assert.ok(
          frame.width - safe.paddingRight + 2 <= frame.width - frame.right,
          'trailing control stays outside the opposite cutout',
        );
        assert.ok(
          frame.width - safe.paddingLeft - safe.paddingRight > leading.width + trailing.width,
        );
        assert.equal(f.control('message-input').value, 'Retained focused text');
        assert.equal(f.input.focused, true);
        assert.deepEqual(f.mounts(), { mounts: 1, unmounts: 0 });
        const viewport = f.style('conversation-composer');
        assert.equal(viewport.left, 0);
        assert.equal(viewport.right, 0);
        assert.equal(
          viewport.bottom,
          -(f.clock.height.value + (Math.max(frame.bottom, 12) - 8) * f.clock.progress.value),
        );
        const wallpaper = f.style('wallpaper');
        assert.equal(wallpaper.left, 0);
        assert.equal(wallpaper.right, 0);
        assert.equal(wallpaper.position, 'absolute');
        await f.act(() =>
          f.control('native-composer-layout').onLayout({ nativeEvent: { layout: { height: 96 } } }),
        );
        assert.equal(
          f.heights.at(-1),
          96,
          'native composer measurement feeds the existing reservation',
        );
      }
      if (searching) {
        await f.act(() => f.control('older-search-result').onPress());
        await f.act(() => f.control('newer-search-result').onPress());
        await f.act(() => f.control('close-conversation-search').onPress());
      } else {
        await f.act(() => f.control('add-attachment').onPress());
        await f.act(() => f.control('send-message').onPress());
        await f.act(() => f.control('cancel-reply').onPress());
      }
      assert.deepEqual(
        f.actions,
        searching ? ['older', 'newer', 'close'] : ['add', 'send', 'cancel-reply'],
      );
      assert.deepEqual(f.errors, []);
    } finally {
      await f.close();
    }
  });
test('no-result AI action stays in the safe trailing edge after rotation and retains its existing action', async () => {
  const f = await composerSafeAreaFixture(true, true);
  try {
    await f.render();
    await f.rotate(rotations[2]!);
    const safe = f.style('message-composer-safe-area');
    assert.equal(safe.paddingRight, rotations[2]!.right + 8);
    assert.equal(f.control('ask-conversation').disabled, false);
    await f.act(() => f.control('ask-conversation').onPress());
    assert.deepEqual(f.actions, ['ask']);
    assert.deepEqual(f.errors, []);
  } finally {
    await f.close();
  }
});
