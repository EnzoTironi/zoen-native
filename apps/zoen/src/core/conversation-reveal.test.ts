import assert from 'node:assert/strict';
import { test } from 'node:test';
import {
  ConversationReveal,
  conversationRevealOffset,
  type ConversationRevealGeometry,
  type ConversationRevealItem,
} from './conversation-reveal.ts';

const geometry = (patch: Partial<ConversationRevealGeometry> = {}): ConversationRevealGeometry => ({
  contentOffset: { y: 360 },
  contentSize: { height: 760 },
  layoutMeasurement: { height: 760 },
  contentInset: { top: 0, bottom: 360 },
  ...patch,
});
const item = (patch: Partial<ConversationRevealItem> = {}): ConversationRevealItem => ({
  y: 0,
  height: 90,
  firstItemOffset: 64,
  childHeight: 200,
  windowHeight: 760,
  topObstruction: 64,
  bottomAligned: true,
  ...patch,
});

test('two short bottom-aligned rows stay above an open keyboard instead of jumping to FlashList max zero', () => {
  const native = geometry();
  const first = item();
  const second = item({ y: 90, height: 110 });
  const physicalMax = Math.max(0, first.childHeight - first.windowHeight + first.firstItemOffset);
  assert.equal(physicalMax, 0);
  assert.equal(conversationRevealOffset(native, first), 360);
  assert.equal(conversationRevealOffset(native, second), 360);
});

test('short chat reveal clamps to the native inset-inclusive end when its last row is hidden', () => {
  assert.equal(
    conversationRevealOffset(geometry({ contentOffset: { y: 0 } }), item({ y: 90, height: 110 })),
    360,
  );
});

test('long chat end includes keyboard and composer exactly once', () => {
  const native = geometry({ contentOffset: { y: 0 }, contentSize: { height: 2400 } });
  const last = item({ y: 2200, height: 136, childHeight: 2336 });
  const target = conversationRevealOffset(native, last);
  assert.equal(target, 2000); // 2400 - 760 + 360, rather than the physical max1640.
  assert.equal(last.y + last.firstItemOffset + last.height - target!, 400);
});

test('a long-chat earlier hit aligns below the header and an oversized hit keeps its beginning visible', () => {
  const native = geometry({ contentOffset: { y: 1800 }, contentSize: { height: 3000 } });
  assert.equal(conversationRevealOffset(native, item({ y: 900, childHeight: 2936 })), 900);
  assert.equal(
    conversationRevealOffset(native, item({ y: 1500, height: 600, childHeight: 2936 })),
    1500,
  );
});

test('closed keyboard, native top inset, and invalid or occluded geometry retain bounded offsets', () => {
  assert.equal(conversationRevealOffset(geometry({ contentInset: { bottom: 0 } }), item()), 0);
  assert.equal(
    conversationRevealOffset(
      geometry({ contentOffset: { y: -20 }, contentInset: { top: 20, bottom: 360 } }),
      item({ y: 0, firstItemOffset: 0, childHeight: 1000, topObstruction: 64 }),
    ),
    -20,
  );
  assert.equal(conversationRevealOffset(geometry({ contentInset: { bottom: 730 } }), item()), null);
  assert.equal(conversationRevealOffset(geometry({ contentSize: { height: NaN } }), item()), null);
});

function fixture() {
  const frames: { run: () => void; cancelled: boolean }[] = [];
  const offsets: number[] = [];
  let current = true;
  let row: ConversationRevealItem | null = item({ y: 90, height: 110 });
  const reveal = new ConversationReveal('test-chat', {
    current: () => current,
    item: () => row,
    frame: (run) => {
      const frame = { run, cancelled: false };
      frames.push(frame);
      return () => {
        frame.cancelled = true;
      };
    },
    scroll: (offset) => offsets.push(offset),
  });
  return {
    reveal,
    offsets,
    frames,
    setCurrent: (value: boolean) => {
      current = value;
    },
    setRow: (value: ConversationRevealItem | null) => {
      row = value;
    },
    ready: (native = geometry({ contentOffset: { y: 0 } })) => {
      reveal.onLayout(native.layoutMeasurement.height);
      reveal.onContentSize(402, native.contentSize.height);
      reveal.onInset({ top: native.contentInset.top ?? 0, bottom: native.contentInset.bottom });
      reveal.onScroll(native);
    },
    frame: () => {
      const frame = frames.at(-1);
      assert.ok(frame, 'native geometry must schedule a real frame');
      if (!frame.cancelled) frame.run();
    },
  };
}

test('a search request waits for actual native scroll geometry with the latest keyboard inset', () => {
  const f = fixture();
  f.ready(geometry({ contentOffset: { y: 0 }, contentInset: { bottom: 100 } }));
  f.reveal.onInset({ top: 0, bottom: 360 });
  f.reveal.request('selected');
  assert.equal(f.frames.length, 0);
  f.reveal.onScroll(geometry({ contentOffset: { y: 0 } }));
  f.frame();
  assert.deepEqual(f.offsets, [360]);
});

test('content and layout changes invalidate an already scheduled frame until native measurements catch up', () => {
  const f = fixture();
  f.ready();
  f.reveal.request('selected');
  const stale = f.frames.at(-1)!;
  f.reveal.onContentSize(402, 900);
  f.reveal.onLayout(800);
  stale.run(); // Even an undelivered cancellation cannot commit stale geometry.
  assert.deepEqual(f.offsets, []);
  f.reveal.onScroll(
    geometry({
      contentOffset: { y: 0 },
      contentSize: { height: 900 },
      layoutMeasurement: { height: 800 },
    }),
  );
  f.frame();
  assert.deepEqual(f.offsets, [460]);
});

test('query replacement, blur/account mismatch and user interruption cannot commit an old reveal', () => {
  for (const cancel of ['query', 'scope', 'drag', 'native-user-scroll'] as const) {
    const f = fixture();
    f.ready();
    f.reveal.request('old-result');
    const stale = f.frames.at(-1)!;
    if (cancel === 'query') {
      f.reveal.request('new-result');
      f.reveal.cancel();
    } else if (cancel === 'scope') f.setCurrent(false);
    else if (cancel === 'drag') f.reveal.cancel();
    else f.reveal.onScroll(geometry({ responderIgnoreScroll: false }));
    stale.run();
    assert.deepEqual(f.offsets, [], cancel);
  }
});

test('unmeasured search thread waits for list commit and samples the latest row layout', () => {
  const f = fixture();
  f.ready();
  f.setRow(null);
  f.reveal.request('selected-thread-message');
  f.frame();
  assert.deepEqual(f.offsets, []);
  f.setRow(item({ y: 90, height: 110 }));
  f.reveal.onCommit();
  f.frame();
  assert.deepEqual(f.offsets, [360]);
  f.reveal.onScroll(geometry());
  f.reveal.onCommit();
  assert.equal(
    f.offsets.length,
    1,
    'a completed request does not fight subsequent native scrolling',
  );
});

test('already visible results finish without a redundant native scroll', () => {
  const f = fixture();
  f.ready(geometry());
  f.reveal.request('selected');
  f.frame();
  assert.deepEqual(f.offsets, []);
});

test('a new explicit search after a user drag uses the latest native offset, not a previous programmatic position', () => {
  const f = fixture();
  f.ready(geometry());
  f.reveal.onScroll(geometry({ contentOffset: { y: 0 }, responderIgnoreScroll: false }));
  f.reveal.request('selected-again');
  f.frame();
  assert.deepEqual(f.offsets, [360]);
});

test('a delayed canonical read cannot create a new reveal after user interruption, blur or query replacement', async () => {
  const f = fixture();
  f.ready();
  const captured = f.reveal.capture();
  let resolve!: () => void;
  const read = new Promise<void>((done) => {
    resolve = done;
  });
  let published = false;
  const completion = read.then(() => {
    if (!f.reveal.isCurrent(captured)) return;
    published = true;
    f.reveal.request('late-result', captured);
  });
  f.reveal.cancel();
  resolve();
  await completion;
  assert.equal(published, false);
  assert.equal(f.frames.length, 0);
  const fresh = f.reveal.capture();
  f.reveal.request('new-user-search', fresh);
  f.frame();
  assert.deepEqual(f.offsets, [360]);
});
