import assert from 'node:assert/strict';
import { test } from 'node:test';
import * as React from 'react';
import { createReactTestRoot } from './react-test-root.fixtures.ts';
import { chatNativeMetricsModule } from './chat-native-metrics.fixtures.ts';
import { ConversationReveal, type ConversationNativeMetrics } from './conversation-reveal.ts';
import type { useChatNativeMetrics } from '../components/use-chat-native-metrics.ts';
import { deferred } from './store.fixtures.ts';

test('actual compiled QA hook observes before search and retires old callbacks across focus/background', async () => {
  const root = createReactTestRoot(),
    records: string[] = [],
    listeners = new Set<() => void>();
  let focused = true,
    current = true,
    searchActive = false;
  const appState = {
    currentState: 'active',
    addEventListener(_event: string, work: () => void) {
      listeners.add(work);
      return {
        remove() {
          listeners.delete(work);
        },
      };
    },
  };
  const hook = chatNativeMetricsModule({
    qa: true,
    native: true,
    appState,
    useFocusEffect(callback) {
      const currentFocus = focused;
      React.useEffect(() => (currentFocus ? callback() : undefined), [callback, currentFocus]);
    },
    emit: (r) => records.push(r),
  }).useChatNativeMetrics;
  const reveal = new ConversationReveal('private scope');
  const packet = (sequence: string): ConversationNativeMetrics => ({
    binding: reveal.nativeBinding,
    sequence,
    geometry: {
      contentOffset: { y: 12 },
      contentSize: { height: 900 },
      layoutMeasurement: { height: 600 },
      contentInset: { top: 80, bottom: 279.333 },
    },
  });
  let value: ReturnType<typeof useChatNativeMetrics> | undefined;
  function Surface() {
    const m = hook({
      reveal,
      focused,
      searchActive,
      current: () => current && focused,
      list: { current: null },
      rows: 0,
      keyboard: { height: { value: 279.333 }, progress: { value: 1 } } as any,
      reply: true,
      editing: false,
    });
    React.useLayoutEffect(() => {
      value = m;
    });
    return null;
  }
  const render = () => root.render(React.createElement(Surface));
  try {
    await render();
    assert.equal(value!.navigationMetrics.binding, reveal.nativeBinding);
    const old = value!.navigationMetrics.changed;
    await root.act(() => old(packet('1')));
    assert.equal(records.length, 1);
    focused = false;
    await render();
    focused = true;
    await render();
    await root.act(() => old(packet('2')));
    assert.equal(records.length, 1);
    searchActive = true;
    await render();
    await root.act(() => value!.navigationMetrics.changed(packet('3')));
    assert.match(records.at(-1)!, /"phase":"search"/);
    searchActive = false;
    await render();
    const pending = deferred<ConversationNativeMetrics | null>();
    let reads = 0;
    value!.navigationMetrics.ref.current = {
      sampleScroll: () => (++reads === 1 ? pending.promise : Promise.resolve(null)),
    };
    searchActive = true;
    await render();
    assert.equal(reads, 1, 'phase transition uses the real native sample port');
    const before = records.length,
      preBackground = value!.navigationMetrics.changed;
    await root.act(() => {
      appState.currentState = 'background';
      for (const work of listeners) work();
    });
    await root.act(() => {
      appState.currentState = 'active';
      for (const work of listeners) work();
    });
    await root.act(async () => {
      pending.resolve(packet('4'));
      await pending.promise;
    });
    await root.act(() => preBackground(packet('4')));
    assert.equal(records.length, before);
    current = false;
    await root.act(() => value!.navigationMetrics.changed(packet('5')));
    assert.equal(records.length, before);
    assert.deepEqual(root.errors, []);
  } finally {
    await root.close();
  }
});
test('actual non-QA hook keeps search-only binding without QA lifecycle/native calls', async () => {
  const root = createReactTestRoot();
  const hook = chatNativeMetricsModule({
    useFocusEffect: (callback) => React.useEffect(callback, [callback]),
  }).useChatNativeMetrics;
  const reveal = new ConversationReveal('private');
  let searching = false,
    binding = 'unset';
  let metrics: ReturnType<typeof useChatNativeMetrics> | undefined;
  function Surface() {
    const m = hook({
      reveal,
      focused: true,
      searchActive: searching,
      current: () => true,
      list: { current: null },
      rows: 0,
      keyboard: {} as any,
      reply: false,
      editing: false,
    });
    React.useLayoutEffect(() => {
      binding = m.navigationMetrics.binding;
      metrics = m;
    });
    return null;
  }
  try {
    await root.render(React.createElement(Surface));
    assert.equal(binding, '');
    metrics!.navigationMetrics.ref.current = {
      sampleScroll: async () => assert.fail('non-QA layout callbacks must never read UIKit'),
    };
    await root.act(() => {
      metrics!.onComposerLayout(279.333);
      metrics!.onInsetRequested({ top: 0, bottom: 581.333 });
    });
    searching = true;
    await root.render(React.createElement(Surface));
    assert.equal(binding, reveal.nativeBinding);
    searching = false;
    await root.render(React.createElement(Surface));
    assert.equal(binding, '');
    assert.deepEqual(root.errors, []);
  } finally {
    await root.close();
  }
});

test('Compiler hook correlates layout intent with a fresh native sample without polling or crossing retirement', async () => {
  const root = createReactTestRoot(),
    records: string[] = [];
  let focused = true,
    current = true,
    reads = 0;
  const hook = chatNativeMetricsModule({
    qa: true,
    native: true,
    appState: { currentState: 'active', addEventListener: () => ({ remove() {} }) },
    useFocusEffect(callback) {
      const active = focused;
      React.useEffect(() => (active ? callback() : undefined), [callback, active]);
    },
    emit: (record) => records.push(record),
  }).useChatNativeMetrics;
  const reveal = new ConversationReveal('private');
  let metrics: ReturnType<typeof useChatNativeMetrics> | undefined;
  function Surface() {
    const value = hook({
      reveal,
      focused,
      searchActive: false,
      current: () => current && focused,
      list: {
        current: {
          getLayout: () => ({ y: 850, height: 50 }),
          getFirstItemOffset: () => 10,
          getChildContainerDimensions: () => ({ height: 900 }),
          getWindowSize: () => ({ height: 600 }),
        } as any,
      },
      rows: 1,
      keyboard: { height: { value: 302 }, progress: { value: 1 } } as any,
      reply: true,
      editing: false,
    });
    React.useLayoutEffect(() => {
      metrics = value;
    });
    return null;
  }
  try {
    await root.render(React.createElement(Surface));
    metrics!.navigationMetrics.ref.current = {
      sampleScroll: async (binding) => ({
        binding,
        sequence: String(++reads),
        geometry: {
          contentOffset: { y: 420.667 },
          contentSize: { height: 900 },
          layoutMeasurement: { height: 600 },
          contentInset: { top: 0, bottom: 393.333 },
        },
      }),
    };
    await root.act(() => {
      metrics!.onComposerLayout(279.333);
      metrics!.onInsetRequested({ top: 0, bottom: 581.333 });
    });
    assert.equal(reads, 2);
    assert.equal(records.length, 1, 'the read preceding the second layout intent is retired');
    const sample = JSON.parse(records[0]!.slice('[ZoenRestore] '.length));
    assert.deepEqual(sample.composer, { height: 279.333 });
    assert.deepEqual(sample.requestedInset, { top: 0, bottom: 581.333 });
    assert.equal(
      sample.native.contentInset.bottom,
      393.333,
      'do not invent native acknowledgement',
    );
    assert.equal(sample.list.lastRowHeight, 50);
    await root.act(() => {
      metrics!.onComposerLayout(279.333);
      metrics!.onComposerLayout(NaN);
      metrics!.onInsetRequested({ top: 0, bottom: 581.333 });
      metrics!.onInsetRequested({ top: -1, bottom: 581.333 });
    });
    assert.equal(reads, 2, 'duplicates and invalid layout cannot poll UIKit');
    current = false;
    await root.act(() => metrics!.onComposerLayout(300));
    assert.equal(reads, 2, 'synchronous owner guard refuses a stale measurement');
    current = true;
    const old = metrics!;
    focused = false;
    await root.render(React.createElement(Surface));
    await root.act(() => {
      old.onComposerLayout(300);
      old.onInsetRequested({ top: 0, bottom: 600 });
    });
    assert.equal(reads, 2, 'retired callbacks do not open another native read');
    assert.deepEqual(root.errors, []);
  } finally {
    await root.close();
  }
});
