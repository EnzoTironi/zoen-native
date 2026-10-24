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
