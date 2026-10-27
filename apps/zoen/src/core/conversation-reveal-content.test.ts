import assert from 'node:assert/strict';
import { test } from 'node:test';
import { ConversationReveal, conversationRevealOffset } from './conversation-reveal.ts';
import {
  conversationRevealContent,
  type ConversationContentLayout,
} from './conversation-reveal-content.ts';

const row = { y: 900, width: 842, height: 640, isHeightMeasured: true, isWidthMeasured: true };
const body: ConversationContentLayout = { y: 42, width: 842, height: 598 };
const geometry = {
  contentOffset: { y: 0 },
  contentSize: { height: 3000 },
  layoutMeasurement: { height: 402 },
  contentInset: { top: 0, bottom: 286 },
};
const base = {
  firstItemOffset: 64,
  childHeight: 2936,
  windowHeight: 402,
  topObstruction: 78,
  bottomAligned: true,
};

test('CORE660 short landscape contact reveals the matching caption line above the keyboard', () => {
  // AX after one older action: row 88.333/136.667, caption wrapper 159.667/44.
  // Native geometry was captured BEFORE that action; this is a reconstruction,
  // not a claim that the capped native trace observed the resulting offset.
  const geometry = {
    contentOffset: { y: 0 },
    contentSize: { height: 414 },
    layoutMeasurement: { height: 402 },
    contentInset: { top: 0, bottom: 289 + 1 / 3 },
  };
  const item = {
    y: 39 + 2 / 3,
    height: 139,
    measured: true,
    firstItemOffset: 86,
    childHeight: 229 + 2 / 3,
    windowHeight: 402,
    topObstruction: 86,
    bottomAligned: true,
  };
  const alignment = 402 - item.childHeight - item.firstItemOffset;
  const matchingLine = { y: item.y + 83 + 2 / 3, height: 23 };
  const old = conversationRevealOffset(geometry, item)!;
  assert.ok(
    matchingLine.y + item.firstItemOffset + alignment - old > 130,
    'whole-body top alignment leaves the caption under the search field',
  );
  const selected = { ...item, anchor: matchingLine };
  const offset = conversationRevealOffset(geometry, selected)!;
  const top = matchingLine.y + item.firstItemOffset + alignment - offset;
  assert.equal(top, 86);
  assert.ok(top + matchingLine.height <= 402 - geometry.contentInset.bottom);
  assert.ok(offset <= 414 - 402 + geometry.contentInset.bottom, 'native clamp remains unchanged');
});

test('pending semantic measurement waits only when the complete body cannot fit', () => {
  const item = { ...base, ...conversationRevealContent(row, { ...body, anchorPending: true }) };
  assert.equal(conversationRevealOffset(geometry, item), null);
  const portrait = {
    ...geometry,
    layoutMeasurement: { height: 1000 },
    contentInset: { bottom: 100 },
  };
  assert.equal(
    conversationRevealOffset(portrait, item),
    conversationRevealOffset(portrait, { ...item, anchorPending: false }),
  );
  assert.equal(
    conversationRevealOffset(portrait, { ...item, anchor: { y: item.y + 100, height: 23 } }),
    conversationRevealOffset(portrait, item),
    'a fitting body preserves portrait alignment',
  );
});

test('a settled oversized body refines a late matching text anchor; drag cancels further work', () => {
  const frames: (() => void)[] = [];
  const offsets: number[] = [];
  let content: ConversationContentLayout = body;
  const reveal = new ConversationReveal('one-chat', {
    current: () => true,
    item: () => ({ ...base, ...conversationRevealContent(row, content) }),
    frame: (run) => {
      frames.push(run);
      return () => {};
    },
    scroll: (offset) => offsets.push(offset),
  });
  const data = [{}];
  reveal.onData(data);
  reveal.onCommit(data);
  reveal.onLayout(402);
  reveal.onContentSize(874, 3000);
  reveal.onInset(geometry.contentInset);
  reveal.onScroll(geometry);
  reveal.request('message');
  frames.at(-1)!();
  assert.deepEqual(offsets, [928]);
  reveal.onScroll({ ...geometry, contentOffset: { y: 928 } });
  frames.at(-1)!();
  content = { ...body, anchor: { y: 300, width: 200, height: 23 } };
  reveal.onItemLayout();
  frames.at(-1)!();
  assert.deepEqual(offsets, [928, 1228], 'late text layout refines the completed body alignment');
  reveal.onScroll({ ...geometry, contentOffset: { y: 1228 } });
  frames.at(-1)!();
  reveal.onItemLayout();
  frames.at(-1)!();
  assert.deepEqual(offsets, [928, 1228], 'already visible text needs no extra native scroll');
  reveal.cancel('user-drag');
  const count = frames.length;
  content = { ...body, anchor: { y: 400, width: 200, height: 23 } };
  reveal.onItemLayout();
  assert.equal(frames.length, count);
});

test('landscape search reveals the message instead of filling the visible strip with its date', () => {
  const oldOffset = conversationRevealOffset(geometry, { ...base, ...row, measured: true })!;
  assert.ok(row.y + base.firstItemOffset + body.y - oldOffset >= 116);
  const selected = { ...base, ...conversationRevealContent(row, body) };
  const offset = conversationRevealOffset(geometry, selected)!;
  assert.equal(selected.y + base.firstItemOffset - offset, 78);
  assert.equal(offset - oldOffset, 42);
  assert.equal(selected.measured, true);
});

test('body measurement wakes a pending seek even without a new scroll or list commit', () => {
  const frames: (() => void)[] = [];
  const offsets: number[] = [];
  let content: ConversationContentLayout | null = null;
  const reveal = new ConversationReveal('one-chat', {
    current: (id) => id === 'message',
    item: () => ({ ...base, ...conversationRevealContent(row, content) }),
    frame: (run) => {
      frames.push(run);
      return () => {};
    },
    scroll: (offset) => offsets.push(offset),
  });
  const data = [{}];
  reveal.onData(data);
  reveal.onCommit(data);
  reveal.onLayout(402);
  reveal.onContentSize(874, 3000);
  reveal.onInset(geometry.contentInset);
  reveal.onScroll(geometry);
  reveal.request('message');
  frames.at(-1)!();
  assert.deepEqual(offsets, [886]);
  content = body;
  reveal.onItemLayout();
  frames.at(-1)!();
  assert.deepEqual(offsets, [886, 928]);
  reveal.cancel('user-drag');
  const count = frames.length;
  reveal.onItemLayout();
  assert.equal(frames.length, count, 'a late layout cannot restart a cancelled search');
});

test('missing, stale-width or stale-height measurements remain provisional after rotation/recycling', () => {
  for (const content of [
    null,
    { ...body, width: 370 },
    { ...body, height: 400 },
    { ...body, y: -1 },
    { ...body, y: NaN },
    { ...body, height: Infinity },
  ]) {
    assert.deepEqual(conversationRevealContent(row, content), {
      y: row.y,
      height: row.height,
      measured: false,
    });
  }
  assert.equal(
    conversationRevealContent({ ...row, isHeightMeasured: false }, body).measured,
    false,
  );
  assert.deepEqual(conversationRevealContent(row, { y: 0, width: 842, height: 640 }), {
    y: 900,
    height: 640,
    measured: true,
  });
});
