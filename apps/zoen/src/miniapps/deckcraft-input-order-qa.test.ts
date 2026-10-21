import assert from 'node:assert/strict';
import { test } from 'node:test';
import { runInNewContext } from 'node:vm';
import {
  deckcraftInputOrderQaEnabled,
  decodeDeckcraftInputOrderQa,
  deckcraftInputOrderQaLabel,
  deckcraftInputOrderQaScript,
} from './deckcraft-input-order-qa.ts';

const token = '10000000-0000-4000-8000-000000000001';
function fixture() {
  type Listener = (event: Record<string, unknown>) => void;
  const listeners = new Map<string, Listener[]>();
  const microtasks: (() => void)[] = [];
  const timers = new Map<number, { work: () => void; delay: number }>();
  const sent: string[] = [];
  let sequence = 0;
  const parent = {};
  const canvas = { tagName: 'CANVAS', parentElement: parent, isConnected: true };
  const input = {
    tagName: 'INPUT',
    type: 'text',
    previousElementSibling: canvas,
    parentElement: parent,
    isConnected: true,
    value: '',
    selectionStart: 0,
    selectionEnd: 0,
  };
  const document = {
    activeElement: input as unknown,
    getElementById: (id: string) => (id === 'deckcraft_canvas' ? canvas : null),
    addEventListener: (kind: string, fn: Listener, options: { capture: boolean }) => {
      const key = `${kind}:${!!options.capture}`;
      listeners.set(key, [...(listeners.get(key) ?? []), fn]);
    },
    removeEventListener: (kind: string, fn: Listener, capture: boolean) => {
      const key = `${kind}:${capture}`;
      listeners.set(
        key,
        (listeners.get(key) ?? []).filter((value) => value !== fn),
      );
    },
  };
  let pagehide: () => void = () => {};
  const window = {
    webkit: {
      messageHandlers: { ReactNativeWebView: { postMessage: (value: string) => sent.push(value) } },
    },
    addEventListener: (name: string, callback: () => void) => {
      if (name === 'pagehide') pagehide = callback;
    },
  };
  runInNewContext(deckcraftInputOrderQaScript(token), {
    document,
    window,
    queueMicrotask: (fn: () => void) => microtasks.push(fn),
    setTimeout: (fn: () => void, delay: number) => {
      timers.set(++sequence, { work: fn, delay });
      return sequence;
    },
    clearTimeout: (id: number) => timers.delete(id),
  });
  const dispatch = (
    kind: string,
    patch: Record<string, unknown> = {},
    targetWork: () => void = () => {},
  ) => {
    const event = {
      type: kind,
      target: input,
      inputType: kind === 'input' || kind === 'beforeinput' ? 'insertText' : '',
      isTrusted: true,
      ...patch,
    };
    for (const listener of listeners.get(`${kind}:true`) ?? []) listener(event);
    // Native dispatch can checkpoint microtasks after the capture callback,
    // before the target listener. A microtask cannot claim after-target order.
    while (microtasks.length) microtasks.shift()!();
    targetWork();
    // Real focus/blur do not bubble. This fixture only simulates DOM order,
    // not native WebKit dispatch or its microtask checkpoint implementation.
    if (kind !== 'focus' && kind !== 'blur')
      for (const listener of listeners.get(`${kind}:false`) ?? []) listener(event);
  };
  const drain = () => {
    while (microtasks.length) microtasks.shift()!();
    let budget = 1024;
    while (timers.size) {
      assert.ok(budget-- > 0, 'bounded diagnostic tasks terminate');
      const [id, task] = [...timers.entries()].sort((a, b) => a[1].delay - b[1].delay)[0]!;
      timers.delete(id);
      task.work();
    }
    return sent.length ? decodeDeckcraftInputOrderQa(sent.at(-1)!, token) : null;
  };
  return {
    input,
    canvas,
    document,
    dispatch,
    drain,
    sent,
    pagehide: () => pagehide(),
    listeners,
    pendingTimers: () => timers.size,
  };
}

test('capture and after-target order survive a native-style microtask checkpoint; task samples retain no content', () => {
  const f = fixture();
  f.input.value = 'SYNTHETIC_PRIVATE_TEXT';
  f.input.selectionStart = f.input.selectionEnd = f.input.value.length;
  f.dispatch('beforeinput');
  f.dispatch('input', {}, () => {
    f.input.value = '';
    f.input.selectionStart = f.input.selectionEnd = 0;
  });
  const state = f.drain()!;
  assert.ok(state);
  assert.deepEqual(
    state.rows.map((row) => [row[1], row[2], row[3]]),
    [
      [1, 0, 0],
      [1, 1, 0],
      [2, 0, 1],
      [2, 1, 1],
      [1, 2, 0],
      [2, 2, 1],
    ],
  );
  assert.equal(state.rows[2]![6], 'SYNTHETIC_PRIVATE_TEXT'.length);
  assert.equal(state.rows[3]![6], 0, 'observe a synchronous buffer clear after the target handler');
  assert.ok(f.sent.every((json) => !json.includes('SYNTHETIC_PRIVATE_TEXT')));
  assert.ok(state.rows.every((row) => row.every(Number.isSafeInteger)));
  assert.ok(!deckcraftInputOrderQaLabel(state).includes('SYNTHETIC_PRIVATE_TEXT'));
});

test('authored trace preserves composition and keydown/up identity, then refuses late delivery after pagehide', () => {
  const f = fixture();
  f.dispatch('compositionstart', { inputType: '', isComposing: true });
  f.dispatch('input', { inputType: 'insertCompositionText', isComposing: true });
  f.dispatch('compositionend', { inputType: '' });
  f.dispatch('keydown', { keyCode: 229, shiftKey: true });
  f.dispatch('keyup', { keyCode: 0 });
  const state = f.drain()!;
  assert.deepEqual(
    state.rows.filter((row) => row[2] === 0).map((row) => [row[3], row[4], row[5], row[10]]),
    [
      [2, 0, 1, 0],
      [1, 3, 1, 0],
      [4, 0, 0, 0],
      [7, 0, 0, 2],
      [8, 0, 0, 1],
    ],
  );
  const sent = f.sent.length;
  f.dispatch('input');
  f.pagehide();
  assert.equal(f.pendingTimers(), 0, 'retirement cancels report and later samples');
  f.drain();
  assert.equal(f.sent.length, sent);
  assert.equal([...f.listeners.values()].flat().length, 0);
});

test('authored non-target/cap/strict decoder rejects extra content and stale owner tokens', () => {
  const f = fixture();
  f.dispatch('input', { target: {} });
  assert.equal(f.drain(), null);
  for (let i = 0; i < 200; i++) f.dispatch('input');
  const state = f.drain()!;
  assert.equal(state.rows.length, 256);
  assert.equal(state.truncated, true);
  const encoded = f.sent.at(-1)!;
  assert.equal(decodeDeckcraftInputOrderQa(encoded, '20000000-0000-4000-8000-000000000002'), null);
  assert.equal(
    decodeDeckcraftInputOrderQa(JSON.stringify({ ...JSON.parse(encoded), text: 'private' }), token),
    null,
  );
  const bad = JSON.parse(encoded);
  bad.rows[0][6] = 'private';
  assert.equal(decodeDeckcraftInputOrderQa(JSON.stringify(bad), token), null);
});

test('authored enablement is default-off and restricted to the exact configured iOS DeckCraft revision', () => {
  const runtime = {
    schema: 1,
    engine: 'deckcraft',
    abi: 'zoen-host-v1',
    revision: 'a31b990d32f2c8562f1588562f922960c3fde48b',
  } as Parameters<typeof deckcraftInputOrderQaEnabled>[2];
  assert.equal(
    deckcraftInputOrderQaEnabled(
      'ios',
      undefined,
      runtime,
      'a31b990d32f2c8562f1588562f922960c3fde48b',
    ),
    false,
  );
  assert.equal(
    deckcraftInputOrderQaEnabled(
      'android',
      '1',
      runtime,
      'a31b990d32f2c8562f1588562f922960c3fde48b',
    ),
    false,
  );
  assert.equal(
    deckcraftInputOrderQaEnabled('ios', '1', runtime, '0000000000000000000000000000000000000000'),
    false,
  );
  assert.equal(
    deckcraftInputOrderQaEnabled('ios', '1', undefined, 'a31b990d32f2c8562f1588562f922960c3fde48b'),
    false,
  );
  assert.equal(
    deckcraftInputOrderQaEnabled('ios', '1', runtime, 'a31b990d32f2c8562f1588562f922960c3fde48b'),
    true,
  );
});
