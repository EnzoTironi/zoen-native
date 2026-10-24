import assert from 'node:assert/strict';
import {
  ConversationReveal,
  type ConversationRevealGeometry,
  type ConversationRevealItem,
  type ConversationNativeMetrics,
} from './conversation-reveal.ts';

export const geometry = (
  patch: Partial<ConversationRevealGeometry> = {},
): ConversationRevealGeometry => ({
  contentOffset: { y: 360 },
  contentSize: { height: 760 },
  layoutMeasurement: { height: 760 },
  contentInset: { top: 0, bottom: 360 },
  ...patch,
});
export const item = (patch: Partial<ConversationRevealItem> = {}): ConversationRevealItem => ({
  y: 0,
  height: 90,
  measured: true,
  firstItemOffset: 64,
  childHeight: 200,
  windowHeight: 760,
  topObstruction: 64,
  bottomAligned: true,
  ...patch,
});

export function fixture(
  trace?: (record: Readonly<Record<string, unknown>>) => void,
  observe?: () => Promise<ConversationNativeMetrics | null>,
) {
  const frames: { run: () => void; cancelled: boolean }[] = [];
  const offsets: number[] = [];
  const animations: boolean[] = [];
  let data: readonly unknown[] = [];
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
    scroll: (offset, animated) => {
      offsets.push(offset);
      animations.push(animated);
    },
    trace,
    observe,
  });
  reveal.onData(data);
  reveal.onCommit(data);
  return {
    reveal,
    offsets,
    animations,
    setData: (next: readonly unknown[]) => {
      data = next;
      reveal.onData(next);
    },
    commit: (committed = data) => reveal.onCommit(committed),
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
