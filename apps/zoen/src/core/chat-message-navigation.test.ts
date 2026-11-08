import assert from 'node:assert/strict';
import type { TimelinePage } from './types';
import { test } from 'node:test';
import { chatSearchReactFixture } from './chat-search-react.fixtures.ts';
import { renderChatScreen } from './composer-recovery-screen.fixtures.ts';
import { find } from './thread-message-selection.fixtures.ts';
import { deferred } from './store.fixtures.ts';
import { plainMessage } from './message-caption.fixtures.ts';
const entry = (id: string) => ({ ...plainMessage(id), id, text_revision: id });

function quotedMessage(context: Parameters<typeof renderChatScreen>[0]) {
  const viewport = find(renderChatScreen(context), 'ConversationViewport')!;
  const content = (viewport.props.children as any)(null, 'Scroll');
  const transition = find(content, 'QuoteLayoutTransition')!;
  const list = find(
    (transition.props.children as any)(
      false,
      () => {},
      () => {},
      false,
    ),
    'FlashList',
  )!;
  const rows = list.props.data as any[];
  return {
    list,
    message: find(
      (list.props.renderItem as any)({ item: rows.at(-1), index: rows.length - 1 }),
      'ConversationTimelineMessage',
    )!,
  };
}

async function fixture(initialBottom = 20, sourceY = 300) {
  const f = await chatSearchReactFixture(false, true);
  f.timeline(async (before) =>
    before !== 'latest'
      ? { entries: [entry('latest')], before: 'latest', has_more: true }
      : { entries: [entry('source')], before: 'source', has_more: false },
  );
  await f.render();
  let sequence = 0;
  const offsets: number[] = [],
    indices: unknown[] = [];
  let geometry = {
    contentOffset: { y: 0 },
    contentSize: { height: 1800 },
    layoutMeasurement: { height: 700 },
    contentInset: { top: 0, bottom: initialBottom },
  };
  const packet = (binding: string) => ({
    binding,
    sequence: String(++sequence),
    geometry: structuredClone(geometry),
  });
  f.current().list.current = {
    getLayout: (index: number) => ({
      x: 0,
      y: index ? 1500 : sourceY,
      width: 402,
      height: 100,
      isHeightMeasured: true,
      isWidthMeasured: true,
    }),
    getFirstItemOffset: () => 0,
    getChildContainerDimensions: () => ({ width: 402, height: 1800 }),
    getWindowSize: () => ({ width: 402, height: 700 }),
    getScrollableNode: () => 42,
    getAbsoluteLastScrollOffset: () => geometry.contentOffset.y,
    scrollToIndex: (request: unknown) => {
      indices.push(request);
      return Promise.resolve();
    },
    scrollToOffset: ({ offset }: { offset: number }) => {
      offsets.push(offset);
      geometry = { ...geometry, contentOffset: { y: offset } };
    },
    scrollToEnd: () => assert.fail('A quote must not follow the bottom'),
  } as any;
  await f.act(() => f.current().setLoadedSpace(f.current().timelineKey));
  f.current().navigationMetrics.ref.current = {
    sampleScroll: async (binding: string) => packet(binding),
  } as any;
  const commit = () =>
    f.act(() => {
      const reveal = f.current().timelineReveal;
      reveal.onLayout(700);
      reveal.onContentSize(402, 1800);
      reveal.onInset(geometry.contentInset);
      f.current().onSearchContentLayout(f.current().highlightedMessageId ?? 'source', {
        y: 0,
        width: 402,
        height: 100,
      });
      reveal.onCommit(f.current().visibleEntries);
    });
  return {
    f,
    offsets,
    indices,
    commit,
    async tapQuote(id = 'source', origin?: string) {
      await f.act(() =>
        (quotedMessage(f.current()).message.props.onQuote as any)(id, undefined, origin),
      );
    },
    async keyboard(bottom: number) {
      geometry = { ...geometry, contentInset: { top: 0, bottom } };
      await f.act(() =>
        f.current().navigationMetrics.changed(packet(f.current().navigationMetrics.binding)),
      );
      await commit();
    },
  };
}

test('actual chat quote paginates then reveals against native keyboard bounds without touching its draft', async () => {
  const { f, offsets, indices, commit, tapQuote } = await fixture(300, 600);
  try {
    f.draft.messageDraft.text = 'Unsent **answer**';
    await f.act(() => {
      f.current().search.open();
      f.current().search.setQuery('prior-search');
      f.current().search.suspend();
    });
    await tapQuote();
    await commit();
    assert.ok(
      f.current().navigationMetrics.binding,
      'normal quote navigation needs the native metrics binding',
    );
    for (let i = 0; i < 8; ++i) await f.frame();
    assert.deepEqual(indices, [], 'use the shared measured reveal, not guessed list indices');
    assert.equal(
      quotedMessage(f.current()).message.props.revealQuery,
      '',
      'a suspended search cannot change the quote anchor',
    );
    assert.deepEqual(offsets, [528]);
    assert.equal(f.current().composerText, 'Unsent **answer**');
    assert.deepEqual(
      f.current().visibleEntries.map((e) => e.id),
      ['source', 'latest'],
    );
    assert.equal(f.calls.filter((c) => c.op === 'timeline' && c.before === 'latest').length, 1);
    assert.equal(f.calls.filter((c) => c.op.startsWith('send_')).length, 0);
    assert.deepEqual(f.errors, []);
  } finally {
    await f.close();
  }
});

test('a visible quote reflows above the opening keyboard, then user drag cancels that intent', async () => {
  const { f, offsets, commit, tapQuote, keyboard } = await fixture();
  try {
    await tapQuote();
    await commit();
    for (let i = 0; i < 8; ++i) await f.frame();
    assert.deepEqual(offsets, [], 'a visible source keeps its native offset');
    await keyboard(400);
    for (let i = 0; i < 8; ++i) await f.frame();
    assert.deepEqual(offsets, [228], 'source must remain above the keyboard');
    await f.act(() => (quotedMessage(f.current()).list.props.onScrollBeginDrag as any)());
    const count = offsets.length;
    await keyboard(500);
    for (let i = 0; i < 8; ++i) await f.frame();
    assert.equal(offsets.length, count);
    assert.equal(f.current().navigationMetrics.binding, '');
    assert.deepEqual(f.errors, []);
  } finally {
    await f.close();
  }
});

for (const boundary of ['reopen', 'access-round-trip'] as const) {
  test(`a pending quote page cannot select or move the conversation after ${boundary}`, async () => {
    const { f, offsets, tapQuote } = await fixture();
    const older = deferred<TimelinePage>();
    try {
      f.timeline(async (before) =>
        before ? older.promise : { entries: [entry('latest')], before: 'latest', has_more: true },
      );
      await tapQuote();
      if (boundary === 'reopen') await f.reopen();
      else {
        const conversations = f.snapshot.conversations;
        await f.act(async () => {
          f.snapshot.conversations = [];
          await f.store.refresh();
          f.snapshot.conversations = conversations;
          await f.store.refresh();
        });
      }
      await f.act(async () => {
        older.resolve({ entries: [entry('source')], before: 'source', has_more: false });
        await older.promise;
      });
      for (let i = 0; i < 4; ++i) await f.frame();
      assert.equal(f.current().highlightedMessageId, null);
      assert.equal(f.current().navigationMetrics.binding, '');
      assert.deepEqual(offsets, []);
      assert.deepEqual(f.errors, []);
    } finally {
      older.resolve({ entries: [], before: null, has_more: false });
      await f.close();
    }
  });
}

test('a newer quote wins when two taps share an outstanding history page', async () => {
  const { f, tapQuote } = await fixture();
  const older = deferred<TimelinePage>();
  try {
    f.timeline(async (before) =>
      before ? older.promise : { entries: [entry('latest')], before: 'latest', has_more: true },
    );
    await tapQuote('source');
    await tapQuote('new-source');
    await f.act(async () => {
      older.resolve({
        entries: [entry('source'), entry('new-source')],
        before: 'source',
        has_more: false,
      });
      await older.promise;
    });
    assert.equal(f.current().highlightedMessageId, 'new-source');
    assert.equal(f.calls.filter((c) => c.op === 'timeline' && c.before === 'latest').length, 1);
    assert.deepEqual(f.errors, []);
  } finally {
    older.resolve({ entries: [], before: null, has_more: false });
    await f.close();
  }
});

test('return from a Chat quote reveals the original message without sending or consuming the draft', async () => {
  const { f, commit, tapQuote, offsets } = await fixture(300, 600);
  try {
    f.draft.messageDraft.text = '**Unsent** answer';
    await tapQuote();
    await commit();
    for (let i = 0; i < 8; ++i) await f.frame();
    assert.equal(f.current().messageNavigation.canReturn, true);
    await f.act(() => f.current().messageNavigation.restore());
    await commit();
    for (let i = 0; i < 8; ++i) await f.frame();
    assert.equal(f.current().highlightedMessageId, 'latest');
    assert.equal(f.current().messageNavigation.canReturn, false);
    assert.deepEqual(offsets, [528, 1400]);
    assert.equal(f.current().composerText, '**Unsent** answer');
    assert.equal(f.calls.filter((c) => c.op.startsWith('send_')).length, 0);
    assert.deepEqual(f.errors, []);
  } finally {
    await f.close();
  }
});

test('nested Chat quote returns unwind one origin per fresh action', async () => {
  const { f, tapQuote } = await fixture();
  try {
    await tapQuote('source', 'latest');
    await tapQuote('latest', 'source');
    const old = f.current().messageNavigation.restore;
    await f.act(() => old());
    assert.equal(f.current().highlightedMessageId, 'source');
    assert.equal(f.current().messageNavigation.canReturn, true);
    await f.act(() => old());
    assert.equal(f.current().highlightedMessageId, 'source');
    assert.equal(f.current().messageNavigation.canReturn, true);
    await f.act(() => f.current().messageNavigation.restore());
    assert.equal(f.current().highlightedMessageId, 'latest');
    assert.equal(f.current().messageNavigation.canReturn, false);
  } finally {
    await f.close();
  }
});

test('user drag ends the Chat seek while preserving its explicit return origin', async () => {
  const { f, tapQuote } = await fixture();
  try {
    await tapQuote();
    await f.act(() => (quotedMessage(f.current()).list.props.onScrollBeginDrag as () => void)());
    assert.equal(f.current().highlightedMessageId, null);
    assert.equal(f.current().messageNavigation.canReturn, true);
    await f.act(() => f.current().messageNavigation.restore());
    assert.equal(f.current().highlightedMessageId, 'latest');
  } finally {
    await f.close();
  }
});

test('reopening the account retires the previous Chat return control', async () => {
  const { f, tapQuote } = await fixture();
  try {
    await tapQuote();
    const old = f.current().messageNavigation.restore;
    await f.reopen();
    const count = f.calls.length;
    await f.act(() => old());
    assert.equal(f.calls.length, count);
    assert.equal(f.current().messageNavigation.canReturn, false);
    assert.equal(f.current().highlightedMessageId, null);
  } finally {
    await f.close();
  }
});

test('pinned bar shares measured chat navigation instead of pushing another conversation', async () => {
  const { f } = await fixture();
  try {
    const viewport = find(renderChatScreen(f.current()), 'ConversationViewport')!;
    const bar = find(viewport.props.overlay, 'PinnedMessageBar')!;
    assert.equal(bar.props.space, f.current().id);
    const revealPinned = bar.props.onReveal;
    assert.ok(typeof revealPinned === 'function');
    await f.act(() => revealPinned('source'));
    assert.equal(f.current().highlightedMessageId, 'source');
    assert.deepEqual(f.errors, []);
  } finally {
    await f.close();
  }
});
