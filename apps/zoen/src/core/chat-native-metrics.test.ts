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
    });
    return null;
  }
  try {
    await root.render(React.createElement(Surface));
    assert.equal(binding, '');
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
