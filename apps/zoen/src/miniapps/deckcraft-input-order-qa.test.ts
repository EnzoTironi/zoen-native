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
  const canvas = {
    tagName: 'CANVAS',
    parentElement: parent,
    isConnected: true,
    nextElementSibling: undefined as unknown,
  };
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
  canvas.nextElementSibling = input;
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
    targetWork: (event: Record<string, unknown>) => void = () => {},
  ) => {
    const event: Record<string, unknown> = {
      type: kind,
      target: input,
      inputType: kind === 'input' || kind === 'beforeinput' ? 'insertText' : '',
      isTrusted: true,
      defaultPrevented: false,
    };
    Object.defineProperties(event, Object.getOwnPropertyDescriptors(patch));
    for (const listener of listeners.get(`${kind}:true`) ?? []) listener(event);
    // Native dispatch can checkpoint microtasks after the capture callback,
    // before the target listener. A microtask cannot claim after-target order.
    while (microtasks.length) microtasks.shift()!();
    targetWork(event);
    // Real focus/blur do not bubble. This fixture only simulates DOM order,
    // not native WebKit dispatch or its microtask checkpoint implementation.
    if (kind !== 'focus' && kind !== 'blur' && !event.cancelBubble)
      for (const listener of listeners.get(`${kind}:false`) ?? []) listener(event);
    return event;
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

test('contact and focus observations preserve DOM order without claiming final prevention or engine ownership', () => {
  const f = fixture();
  f.dispatch(
    'pointerdown',
    { target: f.canvas, pointerType: 'touch', isPrimary: true },
    (event) => {
      event.defaultPrevented = true;
      f.document.activeElement = {};
      f.dispatch('blur', { target: f.input, relatedTarget: f.canvas });
      f.document.activeElement = f.canvas;
      f.dispatch('focus', { target: f.canvas, relatedTarget: f.input });
    },
  );
  f.dispatch(
    'touchstart',
    { target: f.canvas, touches: { length: 1 }, changedTouches: { length: 1 } },
    (event) => {
      event.defaultPrevented = true;
      event.cancelBubble = true;
    },
  );
  const up = f.dispatch('pointerup', {
    target: f.canvas,
    pointerType: 'touch',
    isPrimary: true,
  });
  // eframe's document listener can be registered after this document-bubble
  // observer, so later prevention is not an after-engine assertion.
  up.defaultPrevented = true;
  f.dispatch('touchend', {
    target: f.canvas,
    touches: { length: 0 },
    changedTouches: { length: 1 },
  });
  const state = f.drain()!;
  assert.deepEqual(
    state.rows
      .filter((row) => row[2] === 0)
      .map((row) => [row[1], row[3], row[9], ...row.slice(13)]),
    [
      [1, 9, 1, 2, 0, 2, 1, -1, -1, 0],
      [2, 6, 3, 1, 2, 0, -1, -1, -1, 0],
      [3, 5, 2, 2, 1, 0, -1, -1, -1, 0],
      [4, 12, 2, 2, 0, 0, -1, 1, 1, 0],
      [5, 10, 2, 2, 0, 2, 1, -1, -1, 0],
      [6, 13, 2, 2, 0, 0, -1, 0, 1, 0],
    ],
  );
  assert.equal(state.rows.find((row) => row[1] === 1 && row[2] === 1)![19], 1);
  assert.equal(state.rows.find((row) => row[1] === 5 && row[2] === 1)![19], 0);
  assert.ok(
    !state.rows.some((row) => row[1] === 4 && row[2] === 1),
    'stopped propagation has no bubble sample',
  );
  assert.ok(state.rows.filter((row) => row[2] === 2).every((row) => row[19] === -1));
  assert.ok(
    state.rows
      .filter((row) => row[3]! >= 9)
      .every((row) => row[6] === 0 && row[7] === -1 && row[8] === -1),
  );
});

test('pointer kinds, cancellation and bounded touch counts contain no keys, contacts, coordinates or text', () => {
  const f = fixture();
  Object.defineProperty(f.input, 'value', {
    get: () => {
      throw new Error('contact samples must not read TextAgent text');
    },
  });
  const forbidden = () => {
    throw new Error('private event field was read');
  };
  for (const [pointerType, expected] of [
    ['mouse', 1],
    ['touch', 2],
    ['pen', 3],
    ['__proto__', 4],
  ] as const) {
    f.dispatch('pointercancel', {
      target: f.canvas,
      pointerType,
      isPrimary: false,
      get data() {
        return forbidden();
      },
      get key() {
        return forbidden();
      },
      get pointerId() {
        return forbidden();
      },
      get clientX() {
        return forbidden();
      },
      get clientY() {
        return forbidden();
      },
    });
    const last = f
      .drain()!
      .rows.filter((row) => row[2] === 0)
      .at(-1)!;
    assert.deepEqual([last[3], last[15], last[16]], [11, expected, 0]);
  }
  f.dispatch('touchcancel', {
    target: f.canvas,
    touches: {
      length: 100000,
      get 0() {
        return forbidden();
      },
    },
    changedTouches: {
      length: 2,
      get 0() {
        return forbidden();
      },
    },
  });
  const state = f.drain()!;
  const cancelled = state.rows.filter((row) => row[2] === 0).at(-1)!;
  assert.deepEqual([cancelled[3], cancelled[17], cancelled[18]], [14, 65535, 2]);
  assert.ok(state.rows.every((row) => row.length === 20 && row.every(Number.isSafeInteger)));
});

test('the strict v2 decoder rejects v1 rows, extra fields and inapplicable or misleading scalar values', () => {
  const f = fixture();
  f.dispatch('pointerdown', { target: f.canvas, pointerType: 'touch', isPrimary: true });
  f.drain();
  const encoded = f.sent.at(-1)!;
  const reject = (change: (value: { marker: string; rows: number[][] }) => void) => {
    const value = JSON.parse(encoded);
    change(value);
    assert.equal(decodeDeckcraftInputOrderQa(JSON.stringify(value), token), null);
  };
  reject((v) => {
    v.marker = 'zoen.deckcraft.input-order.qa.v1';
  });
  reject((v) => {
    v.rows[0]!.length = 13;
  });
  reject((v) => {
    v.rows[0]!.push(0);
  });
  for (const [index, bad] of [
    [3, 15],
    [13, 4],
    [14, 1],
    [15, 0],
    [16, 2],
    [17, 1],
    [18, 1],
    [19, -1],
  ] as const)
    reject((v) => {
      v.rows[0]![index] = bad;
    });
  reject((v) => {
    v.rows.find((row) => row[2] === 2)![19] = 0;
  });
});

test('the exact canvas/TextAgent pair is checked on capture, bubble and later-task sampling', () => {
  const missing = fixture();
  missing.canvas.nextElementSibling = {};
  missing.dispatch('pointerdown', {
    target: missing.canvas,
    pointerType: 'touch',
    isPrimary: true,
  });
  assert.equal(missing.drain(), null);
  const f = fixture();
  f.dispatch('pointerdown', { target: f.canvas, pointerType: 'touch', isPrimary: true }, () => {
    f.canvas.nextElementSibling = { ...f.input };
  });
  assert.deepEqual(
    f.drain()!.rows.map((row) => row[2]),
    [0],
    'a different connected sibling cannot inherit samples',
  );
});
