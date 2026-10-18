import assert from 'node:assert/strict';
import { test } from 'node:test';
import { InitialTimelineEnd, initialTimelineEndOffset } from './initial-timeline-end.ts';

test('native initial end adds the composer inset once and clamps short conversations', () => {
  assert.equal(
    initialTimelineEndOffset({ contentHeight: 2667, viewportHeight: 812, bottomInset: 96 }),
    1951,
  );
  assert.equal(
    initialTimelineEndOffset({ contentHeight: 200, viewportHeight: 812, bottomInset: 96 }),
    0,
  );
  assert.equal(
    initialTimelineEndOffset({ contentHeight: 800, viewportHeight: 812, bottomInset: 96 }),
    84,
  );
});

test('native initial end rejects missing, invalid and overflowing geometry', () => {
  const measured = { contentHeight: 2667, viewportHeight: 812, bottomInset: 96 };
  for (const key of ['contentHeight', 'viewportHeight', 'bottomInset'] as const) {
    for (const value of [0, -1, NaN, Infinity, -Infinity])
      assert.equal(initialTimelineEndOffset({ ...measured, [key]: value }), null);
  }
  assert.equal(
    initialTimelineEndOffset({
      contentHeight: Number.MAX_VALUE,
      viewportHeight: 1,
      bottomInset: Number.MAX_VALUE,
    }),
    null,
  );
});

function fixture(options: { anchorUntilInteraction?: boolean } = {}) {
  const frames: { work: () => void; cancelled: boolean }[] = [];
  let current = true;
  let nativeReady = true;
  let scrolled = 0;
  const controller = new InitialTimelineEnd(
    'chat:owner',
    {
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
    },
    options,
  );
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
    ready: (contentHeight = 2763) => {
      controller.onLoad();
      controller.onFooterLayout(96);
      controller.onContentSizeChange(390, contentHeight);
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

function nativeGeometry(
  offset: number,
  contentHeight = 2667,
  viewportHeight = 812,
  bottomInset = 96,
) {
  return {
    contentOffset: { y: offset },
    contentSize: { height: contentHeight },
    layoutMeasurement: { height: viewportHeight },
    contentInset: { bottom: bottomInset },
  };
}

test('an idle native anchor repairs later SDK deviations and dedupes actual end observations', () => {
  const f = fixture({ anchorUntilInteraction: true });
  f.ready(2667);
  f.frames[0].work();
  assert.equal(f.scrolled(), 1);
  f.controller.onNativeScroll(nativeGeometry(1951));
  f.controller.onCommitLayoutEffect();
  f.controller.onNativeScroll(nativeGeometry(1950.5));
  assert.equal(f.frames.length, 1, 'native end and subpoint rounding do not enqueue commands');

  f.controller.onNativeScroll(nativeGeometry(1684.333));
  f.frames[1].work();
  assert.equal(f.scrolled(), 2, 'a later initial-index overwrite is reconciled');
  f.controller.onNativeScroll(nativeGeometry(1951));
  f.controller.onCommitLayoutEffect();
  assert.equal(f.frames.length, 2, 'our own scroll event cannot create a follow loop');
  f.controller.onNativeScroll(nativeGeometry(1684.333));
  f.frames[2].work();
  assert.equal(f.scrolled(), 3, 'the same offset can be a new deviation after reaching the end');
});

test('programmatic and unspecified native scroll markers remain eligible for reconciliation', () => {
  for (const responderIgnoreScroll of [true, undefined]) {
    const f = fixture({ anchorUntilInteraction: true });
    f.ready(2667);
    f.controller.onNativeScroll({ ...nativeGeometry(1684.333), responderIgnoreScroll });
    f.frames.at(-1)!.work();
    assert.equal(f.scrolled(), 1);
    f.controller.onNativeScroll({ ...nativeGeometry(1951), responderIgnoreScroll });
    f.controller.onCommitLayoutEffect();
    assert.equal(f.frames.length, 2, 'an actual end observation stops further correction');
  }
});

test('an explicit native user scroll cancels queued correction before geometry validation', () => {
  const f = fixture({ anchorUntilInteraction: true });
  f.ready(2667);
  f.frames[0].work();
  f.controller.onNativeScroll({ ...nativeGeometry(1684.333), responderIgnoreScroll: true });
  const late = f.frames.at(-1)!;
  f.controller.onNativeScroll({ ...nativeGeometry(NaN), responderIgnoreScroll: false });
  assert.equal(late.cancelled, true);
  late.work();
  f.controller.onNativeScroll({ ...nativeGeometry(1500), responderIgnoreScroll: true });
  f.controller.onNativeScroll(nativeGeometry(1500));
  f.controller.onContentSizeChange(390, 3000);
  f.controller.onFooterLayout(120);
  f.controller.onCommitLayoutEffect();
  assert.equal(f.scrolled(), 1, 'the user scroll cannot be followed by a queued end jump');
  assert.equal(f.frames.length, 2, 'late programmatic events cannot revive the anchor');
});

test('an anchor invalidates old end observations when native content, inset or viewport changes', () => {
  const f = fixture({ anchorUntilInteraction: true });
  f.ready(2300);
  f.controller.onViewportLayout(812);
  f.controller.onNativeScroll(nativeGeometry(1584, 2300));
  for (const frame of f.frames) frame.work();
  assert.equal(f.scrolled(), 0, 'an end observation cancels older queued corrections');

  f.controller.onContentSizeChange(390, 2667);
  f.frames.at(-1)!.work();
  assert.equal(f.scrolled(), 1, 'the previous smaller content end no longer authorizes dedupe');
  f.controller.onNativeScroll(nativeGeometry(1951));
  f.controller.onFooterLayout(120);
  f.frames.at(-1)!.work();
  assert.equal(f.scrolled(), 2, 'a new composer inset invalidates the previous end');
  f.controller.onNativeScroll(nativeGeometry(1975, 2667, 812, 120));
  f.controller.onViewportLayout(700);
  f.frames.at(-1)!.work();
  assert.equal(f.scrolled(), 3, 'a new viewport also invalidates the previous end');
});

test('invalid native observations and a zero initial inset cannot stall a measured anchor', () => {
  const f = fixture({ anchorUntilInteraction: true });
  f.ready(2667);
  f.controller.onNativeScroll(nativeGeometry(1855, 2667, 812, 0));
  f.controller.onNativeScroll(nativeGeometry(NaN));
  f.controller.onNativeScroll(nativeGeometry(1951, Infinity));
  f.frames[0].work();
  assert.equal(f.scrolled(), 1);
  f.controller.onNativeScroll(nativeGeometry(1951));
  f.controller.onNativeScroll(nativeGeometry(Infinity));
  f.controller.onCommitLayoutEffect();
  assert.equal(f.frames.length, 1, 'invalid observations do not overwrite a valid native end');
});

test('first user intent permanently releases an anchor even after successful initial positioning', () => {
  const f = fixture({ anchorUntilInteraction: true });
  f.ready(2667);
  f.frames[0].work();
  f.controller.onNativeScroll(nativeGeometry(1684.333));
  const late = f.frames.at(-1)!;
  f.controller.cancel();
  assert.equal(late.cancelled, true);
  late.work();
  f.controller.onNativeScroll(nativeGeometry(1500));
  f.controller.onContentSizeChange(390, 3000);
  f.controller.onViewportLayout(700);
  f.controller.onFooterLayout(120);
  f.controller.onCommitLayoutEffect();
  assert.equal(f.scrolled(), 1);
  assert.equal(f.frames.length, 2);
});

test('an active anchor rechecks live ownership and cannot revive when the old scope returns', () => {
  const f = fixture({ anchorUntilInteraction: true });
  f.ready(2667);
  f.frames[0].work();
  f.setCurrent(false);
  f.controller.onNativeScroll(nativeGeometry(1684.333));
  f.frames.at(-1)!.work();
  f.setCurrent(true);
  f.controller.onNativeScroll(nativeGeometry(1500));
  f.controller.onCommitLayoutEffect();
  assert.equal(f.scrolled(), 1);
  assert.equal(f.frames.length, 2);
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
