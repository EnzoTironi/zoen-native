import assert from 'node:assert/strict';
import { test } from 'node:test';
import { conversationRevealOffset } from './conversation-reveal.ts';
import { fixture, geometry, item } from './conversation-reveal.fixtures.ts';

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

test('a search seeks with actual native bounds and refines after the latest keyboard inset is acknowledged', () => {
  const f = fixture();
  f.ready(geometry({ contentOffset: { y: 0 }, contentInset: { bottom: 100 } }));
  f.reveal.onInset({ top: 0, bottom: 360 });
  f.reveal.request('selected');
  f.frame();
  assert.deepEqual(f.offsets, [100], 'the desired inset cannot replace measured native bounds');
  f.reveal.onScroll(geometry({ contentOffset: { y: 0 } }));
  f.frame();
  assert.deepEqual(f.offsets, [100, 360]);
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
  f.commit();
  f.frame();
  assert.deepEqual(f.offsets, [360]);
  f.reveal.onScroll(geometry());
  f.commit();
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

test('same-chat pagination rejects a valid old measured index until the exact new entries commit', () => {
  const f = fixture();
  const oldData = [{ id: 'previous-row' }];
  const expandedData = [{ id: 'older-row' }, ...oldData];
  f.setData(oldData);
  f.commit(oldData);
  f.ready();
  f.setData(expandedData);
  // getLayout still returns non-null, fully measured old-index geometry.
  f.setRow(item({ y: 90, height: 110, measured: true }));
  f.reveal.request('selected-after-pagination');
  assert.equal(f.frames.length, 0);
  f.commit(oldData);
  assert.equal(f.frames.length, 0);
  f.commit(expandedData);
  f.frame();
  assert.deepEqual(f.offsets, [360]);
});

test('estimated engagement remains pending and refines the real variable-height row without retargeting native animation frames', () => {
  const f = fixture();
  const native = geometry({ contentOffset: { y: 0 }, contentSize: { height: 3000 } });
  f.ready(native);
  f.setRow(item({ y: 900, childHeight: 2936, measured: false }));
  f.reveal.request('newly-loaded-recipe');
  f.frame();
  assert.deepEqual(f.offsets, [900]);
  assert.deepEqual(f.animations, [false]);
  f.reveal.onScroll({ ...native, contentOffset: { y: 500 } });
  f.frame();
  assert.deepEqual(f.offsets, [900]);
  f.setRow(item({ y: 1300, height: 180, childHeight: 2936, measured: true }));
  f.commit();
  f.frame();
  assert.deepEqual(f.offsets, [900, 1300]);
  assert.deepEqual(f.animations, [false, true]);
  for (const y of [900, 1100]) {
    f.reveal.onScroll({ ...native, contentOffset: { y } });
    f.frame();
  }
  assert.deepEqual(f.offsets, [900, 1300]);
  f.reveal.onScroll({ ...native, contentOffset: { y: 1300 } });
  f.frame();
  f.setRow(item({ y: 1800, childHeight: 2936, measured: true }));
  f.commit();
  assert.deepEqual(
    f.offsets,
    [900, 1300],
    'acknowledged target retires after its measured SDK commit',
  );
});

test('a seek cannot retire against old measured flags before the newly engaged target commits', () => {
  const f = fixture();
  const native = geometry({ contentOffset: { y: 0 }, contentSize: { height: 3000 } });
  f.ready(native);
  f.setRow(item({ y: 900, childHeight: 2936 }));
  f.reveal.request('recipe');
  f.frame();
  f.reveal.onScroll({ ...native, contentOffset: { y: 900 } });
  f.frame();
  f.setRow(item({ y: 1300, childHeight: 2936 }));
  f.commit();
  f.frame();
  assert.deepEqual(f.offsets, [900, 1300]);
});

test('a provisional hit never completes merely because its estimated bounds are visible', () => {
  const f = fixture();
  f.ready(geometry());
  f.setRow(item({ y: 90, height: 110, measured: false }));
  f.reveal.request('unmeasured');
  f.frame();
  assert.deepEqual(f.offsets, []);
  f.setRow(item({ y: 700, height: 110, childHeight: 1500, measured: true }));
  f.ready(geometry({ contentSize: { height: 1564 } }));
  f.commit();
  f.frame();
  assert.deepEqual(f.offsets, [700]);
});

test('drag or query replacement cancels all later variable-height refinements and native acknowledgements', () => {
  for (const interrupt of ['drag', 'query'] as const) {
    const f = fixture();
    const native = geometry({ contentOffset: { y: 0 }, contentSize: { height: 3000 } });
    f.ready(native);
    f.setRow(item({ y: 900, childHeight: 2936, measured: false }));
    f.reveal.request('old-hit');
    f.frame();
    if (interrupt === 'drag')
      f.reveal.onScroll({ ...native, contentOffset: { y: 500 }, responderIgnoreScroll: false });
    else f.reveal.cancel();
    f.setRow(item({ y: 1300, childHeight: 2936, measured: true }));
    f.commit();
    f.reveal.onScroll({ ...native, contentOffset: { y: 900 } });
    f.frame();
    assert.deepEqual(f.offsets, [900], interrupt);
  }
});

test('newly committed rows cannot finish at a stale native content limit while still behind the keyboard', () => {
  const f = fixture();
  f.ready(geometry());
  const expanded = [{ id: 'older-recipe' }, { id: 'previous-row' }];
  f.setData(expanded);
  f.setRow(item({ y: 900, height: 110, childHeight: 1500, measured: true }));
  f.commit(expanded);
  f.reveal.request('older-recipe');
  f.frame();
  // The old content height clamps the desired offset to its current 360 even
  // though the new, measured row starts at 604, below the visible bottom 400.
  assert.deepEqual(f.offsets, []);
  f.reveal.onContentSize(402, 1564);
  f.frame();
  assert.deepEqual(f.offsets, [900], 'the actual native size event supplies the new range');
  f.reveal.onScroll(geometry({ contentSize: { height: 1564 } }));
  f.frame();
  assert.deepEqual(f.offsets, [900], 'the pending target seeks when native content catches up');
  f.commit(expanded);
  f.reveal.onScroll(geometry({ contentSize: { height: 1564 }, contentOffset: { y: 900 } }));
  f.frame();
  f.setRow(item({ y: 1200, height: 110, childHeight: 1500, measured: true }));
  f.commit(expanded);
  assert.deepEqual(f.offsets, [900], 'only the genuinely visible target can retire');
});

test('an oversized measured result can finish with its beginning aligned above the keyboard', () => {
  const f = fixture();
  f.ready(geometry({ contentSize: { height: 3000 }, contentOffset: { y: 900 } }));
  f.setRow(item({ y: 900, height: 600, childHeight: 2936, measured: true }));
  f.reveal.request('oversized');
  f.frame();
  f.setRow(item({ y: 1300, height: 600, childHeight: 2936, measured: true }));
  f.commit();
  assert.deepEqual(f.offsets, []);
});

test('optional QA traces expose geometry without scope or raw targets and cannot change scrolling', () => {
  const records: Readonly<Record<string, unknown>>[] = [];
  const f = fixture((record) => records.push(record));
  f.ready();
  f.reveal.request('public-target-for-diagnostic');
  f.frame();
  const sample = records.find((record) => record.event === 'sample');
  assert.ok(sample);
  assert.deepEqual(sample.item, item({ y: 90, height: 110 }));
  assert.equal(sample.atTarget, false);
  assert.equal(sample.ready, true);
  assert.deepEqual(sample.waiting, []);
  assert.ok(records.some((record) => record.event === 'issue' && record.offset === 360));
  assert.equal(JSON.stringify(records).includes('test-chat'), false);
  assert.equal(JSON.stringify(records).includes('public-target-for-diagnostic'), false);
  const throwing = fixture(() => {
    throw new Error('diagnostic unavailable');
  });
  throwing.ready();
  throwing.reveal.request('another-target');
  throwing.frame();
  assert.deepEqual(throwing.offsets, f.offsets);
});

test('native content-size measurement refines the v72 seek without requiring another same-offset scroll event', () => {
  const f = fixture();
  const native = geometry({
    contentOffset: { y: 3213.6666666666665 },
    contentSize: { height: 3573.333251953125 },
    layoutMeasurement: { height: 812 },
    contentInset: { top: 0, bottom: 452.3333435058594 },
  });
  const entries = Array.from({ length: 33 }, (_, index) => ({ id: `entry-${index}` }));
  f.setData(entries);
  f.commit(entries);
  f.ready(native);
  const estimated = item({
    y: 1456,
    height: 100.53333333333332,
    measured: false,
    firstItemOffset: 62,
    childHeight: 3499.200000000002,
    windowHeight: 812,
    topObstruction: 62,
  });
  f.setRow(estimated);
  f.reveal.request('recipe');
  f.frame();
  assert.deepEqual(f.offsets, [1456]);
  f.reveal.onInset({ top: 0, bottom: 431.3333435058594 });
  const acknowledged = {
    ...native,
    contentOffset: { y: 1456 },
    contentInset: { top: 0, bottom: 431.3333435058594 },
  };
  f.reveal.onScroll(acknowledged);
  f.commit(entries);
  // The native trace ends with this size event and no further scroll packet.
  // The now-measured fixture row needs a different offset than its estimate.
  f.setRow({ ...estimated, y: 1972, height: 217, childHeight: 4018.66650390625, measured: true });
  f.reveal.onContentSize(402, 4092.66650390625);
  f.frame();
  assert.deepEqual(f.offsets, [1456, 1972]);
  assert.deepEqual(f.animations, [false, true]);
  f.setRow({ ...estimated, y: 2100, height: 217, childHeight: 4018.66650390625, measured: true });
  f.commit(entries);
  f.frame();
  assert.deepEqual(
    f.offsets,
    [1456, 1972, 2100],
    'content measurement is not an offset acknowledgement',
  );
});

test('a late scroll packet cannot replace the independently measured native content range', () => {
  const f = fixture();
  const native = geometry({
    contentOffset: { y: 1456 },
    contentSize: { height: 3573.333251953125 },
    layoutMeasurement: { height: 812 },
    contentInset: { top: 0, bottom: 431.3333435058594 },
  });
  f.ready(native);
  f.reveal.onContentSize(402, 4092.66650390625);
  f.setRow(
    item({
      y: 3900,
      height: 118,
      childHeight: 4018.66650390625,
      windowHeight: 812,
      firstItemOffset: 62,
      topObstruction: 62,
    }),
  );
  f.reveal.onScroll({ ...native, contentOffset: { y: 1500 } });
  f.reveal.request('last-hit');
  f.frame();
  assert.deepEqual(f.offsets, [4092.66650390625 - 812 + 431.3333435058594]);
});
