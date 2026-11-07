import assert from 'node:assert/strict';
import { test } from 'node:test';
import { threadNavigationFixture } from './thread-navigation-react.fixtures.ts';
import { deferred } from './store.fixtures.ts';
import type { TimelineEntry } from './types';
import { find } from './thread-message-selection.fixtures.ts';

async function frames(f: Awaited<ReturnType<typeof threadNavigationFixture>>) {
  for (let i = 0; i < 8; ++i) await f.frame();
}

test('compiled Thread quote binds its real header to native geometry without changing the draft', async () => {
  const f = await threadNavigationFixture();
  try {
    await f.tap('source');
    await f.commit();
    await frames(f);
    assert.equal(f.ui().header.scrollViewTag, 42);
    assert.ok(f.ui().header.metrics.binding);
    assert.deepEqual(f.offsets, [528]);
    assert.deepEqual(f.indices, []);
    assert.equal(f.ui().message(1).highlighted, true);
    assert.equal(
      find(f.ui().viewport.composer, 'MessageComposer')!.props.value,
      f.draft.messageDraft.text,
    );
    assert.deepEqual(f.calls, []);
    assert.deepEqual(f.root.errors, []);
  } finally {
    await f.close();
  }
});

test('a visible Thread origin stays put until the keyboard hides it, and dragging cancels the reveal', async () => {
  const f = await threadNavigationFixture();
  try {
    await f.keyboard(0);
    await f.tap('source');
    await f.commit();
    await frames(f);
    assert.deepEqual(f.offsets, []);
    await f.keyboard(300);
    await frames(f);
    assert.deepEqual(f.offsets, [528]);
    await f.root.act(() => f.ui().list.onScrollBeginDrag());
    const count = f.offsets.length;
    await f.keyboard(500);
    await frames(f);
    assert.equal(f.offsets.length, count);
    assert.equal(f.ui().header.metrics.binding, '');
    assert.equal(f.ui().message(1).highlighted, false);
    assert.deepEqual(f.root.errors, []);
  } finally {
    await f.close();
  }
});

test('Thread deep links wait for measured commit rather than initial index placement', async () => {
  const f = await threadNavigationFixture('latest');
  try {
    assert.equal(f.ui().list.initialScrollIndex, undefined);
    await f.commit();
    await frames(f);
    assert.deepEqual(f.offsets, [1128]);
    assert.deepEqual(f.indices, []);
    assert.equal(f.ui().message(2).highlighted, true);
  } finally {
    await f.close();
  }
});

test('a selected album child becomes an individually measured Thread target', async () => {
  const f = await threadNavigationFixture(undefined, true);
  try {
    assert.equal(f.ui().list.data.length, 2);
    await f.tap('latest');
    await f.commit();
    await frames(f);
    assert.equal(f.ui().list.data.length, 3);
    assert.equal(f.ui().message(2).highlighted, true);
    assert.deepEqual(f.offsets, [1128]);
    assert.deepEqual(f.indices, []);
  } finally {
    await f.close();
  }
});

test('a quote outside loaded Thread entries uses the canonical message destination', async () => {
  const f = await threadNavigationFixture();
  try {
    await f.tap('elsewhere');
    assert.deepEqual(f.routes, [
      {
        pathname: '/thread/[space]/[root]',
        params: { space: 'read-chat', root: 'another-root', jump: 'elsewhere' },
      },
    ]);
    assert.deepEqual(f.root.errors, []);
  } finally {
    await f.close();
  }
});

for (const boundary of ['root', 'focus', 'access-round-trip'] as const) {
  test(`a pending Thread quote cannot navigate after ${boundary} changes`, async () => {
    const f = await threadNavigationFixture();
    const response = deferred<TimelineEntry>();
    try {
      f.read(() => response.promise);
      const old = f.ui().message().onQuote;
      await f.tap('elsewhere');
      if (boundary === 'root') await f.changeRoot();
      else if (boundary === 'focus') {
        await f.blur();
        await f.focus();
      } else {
        const chats = f.snapshot.conversations;
        await f.root.act(async () => {
          f.snapshot.conversations = [];
          await f.store.refresh();
          f.snapshot.conversations = chats;
          await f.store.refresh();
        });
      }
      await f.root.act(async () => {
        response.resolve({ ...f.entries[1]!, id: 'elsewhere', in_thread: 'another-root' });
        await response.promise;
      });
      assert.deepEqual(f.routes, []);
      assert.deepEqual(f.offsets, []);
      if (boundary === 'root') {
        await f.root.act(() => old('source'));
        assert.equal(f.ui().message(1).highlighted, false);
        await f.tap('other-child');
        assert.equal(f.ui().message(1).highlighted, true);
        await f.root.act(() => old('source'));
        assert.equal(
          f.ui().message(1).highlighted,
          true,
          'old-root callbacks cannot replace a fresh target',
        );
      }
      assert.deepEqual(f.root.errors, []);
    } finally {
      response.resolve(f.entries[0]!);
      await f.close();
    }
  });
}

test('bulk selection cancels Thread navigation without reviving its route jump afterwards', async () => {
  const f = await threadNavigationFixture('source');
  try {
    await f.commit();
    await frames(f);
    await f.selection(true);
    await f.selection(false);
    assert.equal(f.ui().header.metrics.binding, '');
    assert.equal(f.ui().message(1).highlighted, false);
    await f.tap('latest');
    await f.commit();
    await frames(f);
    assert.equal(f.ui().message(2).highlighted, true);
    assert.deepEqual(f.root.errors, []);
  } finally {
    await f.close();
  }
});

test('an inaccessible quote origin never issues a Core read or guesses a destination', async () => {
  const f = await threadNavigationFixture();
  try {
    await f.tap('elsewhere', 'private-other-chat');
    assert.deepEqual(f.calls, []);
    assert.deepEqual(f.routes, []);
    const alert = find(f.ui().viewport.composer, 'Text');
    assert.equal(alert?.props.accessibilityRole, 'alert');
    assert.equal(alert?.props.children, 'chat.messageUnavailable');
    assert.deepEqual(f.root.errors, []);
  } finally {
    await f.close();
  }
});

test('late old-root native layout events cannot replace the new Thread binding', async () => {
  const f = await threadNavigationFixture('source');
  try {
    const oldLoad = f.ui().list.onLoad,
      oldLayout = f.ui().viewport.onNativeLayout;
    await f.changeRoot();
    await f.root.act(() => f.ui().list.onLoad());
    await f.tap('other-child');
    await f.commit();
    assert.equal(f.ui().header.scrollViewTag, 42);
    await f.root.act(() => {
      oldLoad();
      oldLayout(700);
    });
    assert.equal(f.ui().header.scrollViewTag, 42);
    await frames(f);
    assert.equal(f.ui().message(1).highlighted, true);
    assert.deepEqual(f.offsets, [528]);
    assert.deepEqual(f.root.errors, []);
  } finally {
    await f.close();
  }
});

test('Thread return reveals the original reply and consumes only its own bookmark', async () => {
  const f = await threadNavigationFixture();
  try {
    await f.tap('source', undefined, 'latest');
    await f.commit();
    await frames(f);
    const control = f.ui().viewport.navigation.props.navigation;
    assert.equal(control.canReturn, true);
    await f.root.act(() => control.restore());
    await f.commit();
    await frames(f);
    assert.equal(f.ui().message(2).highlighted, true);
    assert.equal(f.ui().viewport.navigation.props.navigation.canReturn, false);
    assert.deepEqual(f.offsets, [528, 1128]);
    assert.deepEqual(f.calls, []);
    assert.equal(
      find(f.ui().viewport.composer, 'MessageComposer')!.props.value,
      f.draft.messageDraft.text,
    );
  } finally {
    await f.close();
  }
});

test('a return button from another Thread cannot replace its fresh navigation', async () => {
  const f = await threadNavigationFixture();
  try {
    await f.tap('source', undefined, 'latest');
    const old = f.ui().viewport.navigation.props.navigation;
    await f.changeRoot();
    await f.tap('different-root', undefined, 'other-child');
    assert.equal(f.ui().message(0).highlighted, true);
    await f.root.act(() => old.restore());
    assert.equal(f.ui().message(0).highlighted, true);
    assert.equal(f.ui().viewport.navigation.props.navigation.canReturn, true);
  } finally {
    await f.close();
  }
});

test('an unavailable return origin removes its bookmark without changing the draft or writing Core', async () => {
  const f = await threadNavigationFixture();
  try {
    await f.tap('source', undefined, 'latest');
    await f.remove('latest');
    const control = f.ui().viewport.navigation.props.navigation;
    await f.root.act(async () => {
      await assert.rejects(control.restore(), /chat.thisMessageIsNoLongerInThisDevice/);
    });
    assert.equal(f.ui().viewport.navigation.props.navigation.canReturn, false);
    assert.deepEqual(f.calls, []);
    assert.equal(
      find(f.ui().viewport.composer, 'MessageComposer')!.props.value,
      f.draft.messageDraft.text,
    );
    assert.deepEqual(f.root.errors, []);
  } finally {
    await f.close();
  }
});
