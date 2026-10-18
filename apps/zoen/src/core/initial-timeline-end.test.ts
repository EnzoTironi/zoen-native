import assert from 'node:assert/strict';
import { test } from 'node:test';
import { InitialTimelineEnd } from './initial-timeline-end.ts';

function fixture() {
  const frames: { work: () => void; cancelled: boolean }[] = [];
  let current = true;
  let nativeReady = true;
  let scrolled = 0;
  const controller = new InitialTimelineEnd('chat:owner', {
    current: () => current,
    frame: (work) => {
      const frame = { work, cancelled: false };
      frames.push(frame);
      return () => {
        frame.cancelled = true;
      };
    },
    scroll: () => {
      if (!nativeReady) return false;
      scrolled++;
      return true;
    },
  });
  return {
    controller,
    frames,
    scrolled: () => scrolled,
    setCurrent: (value: boolean) => {
      current = value;
    },
    setNativeReady: (value: boolean) => {
      nativeReady = value;
    },
    ready: () => {
      controller.onLoad();
      controller.onFooterLayout(96);
      controller.onContentSizeChange(390, 2763);
    },
  };
}

test('initial end waits for actual footer, loaded rows and native content in every event order', () => {
  const orders = [
    ['load', 'footer', 'content'],
    ['load', 'content', 'footer'],
    ['footer', 'load', 'content'],
    ['footer', 'content', 'load'],
    ['content', 'load', 'footer'],
    ['content', 'footer', 'load'],
  ] as const;
  for (const order of orders) {
    const f = fixture();
    const signals = {
      load: f.controller.onLoad,
      footer: () => f.controller.onFooterLayout(96),
      content: () => f.controller.onContentSizeChange(390, 2763),
    };
    order.forEach((signal, index) => {
      signals[signal]();
      assert.equal(f.frames.length, index === 2 ? 1 : 0, order.join(' → '));
      assert.equal(f.scrolled(), 0);
    });
    f.frames[0].work();
    assert.equal(f.scrolled(), 1);
  }
});

test('zero or invalid geometry cannot authorize initial positioning', () => {
  const f = fixture();
  f.controller.onLoad();
  for (const height of [0, -1, NaN, Infinity]) f.controller.onFooterLayout(height);
  f.controller.onContentSizeChange(390, 2763);
  assert.equal(f.frames.length, 0);
  f.controller.onFooterLayout(96);
  assert.equal(f.frames.length, 1);

  const g = fixture();
  g.controller.onLoad();
  g.controller.onFooterLayout(96);
  for (const [width, height] of [
    [0, 100],
    [100, 0],
    [-1, 100],
    [100, NaN],
    [Infinity, 100],
  ])
    g.controller.onContentSizeChange(width, height);
  g.controller.onCommitLayoutEffect();
  assert.equal(g.frames.length, 0);
});

test('later content and SDK layout commits replace queued work and fence already-dispatched frames', () => {
  const f = fixture();
  f.ready();
  f.controller.onContentSizeChange(390, 2807);
  f.controller.onCommitLayoutEffect();
  assert.equal(f.frames.length, 3);
  assert.equal(f.frames[0].cancelled, true);
  assert.equal(f.frames[1].cancelled, true);
  // Native/JS scheduling can deliver an old callback after cancellation.
  f.frames[0].work();
  f.frames[1].work();
  assert.equal(f.scrolled(), 0);
  f.frames[2].work();
  assert.equal(f.scrolled(), 1);
  f.controller.onContentSizeChange(390, 3000);
  f.controller.onFooterLayout(120);
  f.controller.onCommitLayoutEffect();
  f.controller.onLoad();
  assert.equal(f.frames.length, 3, 'completed initial positioning never becomes ongoing follow');
});

test('cancelled initial work cannot revive after a dispatched frame or late layout', () => {
  const f = fixture();
  f.ready();
  f.controller.cancel();
  assert.equal(f.frames[0].cancelled, true);
  f.frames[0].work();
  f.controller.onCommitLayoutEffect();
  f.controller.onContentSizeChange(390, 3000);
  assert.equal(f.scrolled(), 0);
  assert.equal(f.frames.length, 1);
});

test('execution checks live ownership even when a scope change overtakes effect cleanup', () => {
  const f = fixture();
  f.ready();
  f.setCurrent(false);
  f.frames[0].work();
  assert.equal(f.scrolled(), 0);
  f.setCurrent(true);
  f.controller.onCommitLayoutEffect();
  assert.equal(f.frames.length, 1, 'a stale initial request cannot revive after ownership returns');
});

test('a temporarily absent native ref waits for a later real layout instead of spinning frames', () => {
  const f = fixture();
  f.setNativeReady(false);
  f.ready();
  f.frames[0].work();
  assert.equal(f.scrolled(), 0);
  assert.equal(f.frames.length, 1);
  f.setNativeReady(true);
  f.controller.onCommitLayoutEffect();
  f.frames[1].work();
  assert.equal(f.scrolled(), 1);
});
