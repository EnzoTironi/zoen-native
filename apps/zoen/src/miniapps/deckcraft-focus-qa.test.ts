import assert from 'node:assert/strict';
import test from 'node:test';
import { runInNewContext } from 'node:vm';
import { DECKCRAFT_ENGINE_REVISION } from './canonical-binary.ts';
import {
  deckcraftFocusQaEnabled,
  deckcraftFocusQaLabel,
  deckcraftFocusQaScript,
  decodeDeckcraftFocusQa,
} from './deckcraft-focus-qa.ts';
import type { RuntimePackage } from './runtime-package.ts';

const session = '00000000-0000-4000-8000-111111111111';
const snapshot = {
  marker: 'zoen.deckcraft.focus.qa.v1',
  session,
  slot: 'childCanvas',
  event: 'focus',
  activeTag: 'INPUT',
  inputCount: 1,
  hasFocus: true,
  canvasFocused: false,
  inputFocused: true,
  isTop: false,
  inert: false,
  disabled: false,
};

test('QA is explicit default-off, iOS-only and exact managed DeckCraft only', () => {
  const runtime = {
    schema: 1,
    engine: 'deckcraft',
    abi: 'zoen-host-v1',
    revision: DECKCRAFT_ENGINE_REVISION,
  } as RuntimePackage;
  assert.equal(deckcraftFocusQaEnabled('ios', undefined, runtime), false);
  assert.equal(deckcraftFocusQaEnabled('ios', 'true', runtime), false);
  assert.equal(deckcraftFocusQaEnabled('android', '1', runtime), false);
  assert.equal(deckcraftFocusQaEnabled('ios', '1', undefined), false);
  assert.equal(
    deckcraftFocusQaEnabled('ios', '1', { ...runtime, revision: '0'.repeat(40) }),
    false,
  );
  assert.equal(deckcraftFocusQaEnabled('ios', '1', runtime), true);
});

test('strict scalar decoder rejects extra payload, malformed enums/counts and a retired document token', () => {
  const decoded = decodeDeckcraftFocusQa(JSON.stringify(snapshot), session);
  assert.ok(decoded);
  assert.equal('session' in decoded, false);
  assert.equal('marker' in decoded, false);
  assert.ok(Object.isFrozen(decoded));
  for (const changed of [
    { ...snapshot, value: 'never admit text' },
    { ...snapshot, url: 'never admit URLs' },
    { ...snapshot, key: 'never admit keys' },
    { ...snapshot, hasFocus: 'true' },
    { ...snapshot, activeTag: 'private-document-title' },
    { ...snapshot, event: 'arbitrary-event' },
    { ...snapshot, slot: 'top' },
    { ...snapshot, inputCount: -1 },
    { ...snapshot, inputCount: 1.5 },
    { ...snapshot, inputCount: 9 },
    { ...snapshot, marker: 'different-channel' },
    { ...snapshot, session: '00000000-0000-4000-8000-999999999999' },
  ])
    assert.equal(decodeDeckcraftFocusQa(JSON.stringify(changed), session), null);
  assert.equal(decodeDeckcraftFocusQa(' '.repeat(769), session), null);
  assert.equal(decodeDeckcraftFocusQa('null', session), null);
  assert.equal(decodeDeckcraftFocusQa('[]', session), null);
  assert.equal(decodeDeckcraftFocusQa('{', session), null);
  assert.equal(decodeDeckcraftFocusQa(null, session), null);
  assert.equal(decodeDeckcraftFocusQa(snapshot, session), null);
  const label = deckcraftFocusQaLabel({ top: null, childCanvas: decoded });
  assert.match(label, /childCanvas: focus tag=INPUT focus=1 canvas=0 input=1/);
  assert.equal(label.includes(session), false);
});

test('actual generated observer reads only fixed focus scalars, clamps inputs and retires delayed work', () => {
  const sent: string[] = [];
  const documentEvents = new Map<string, () => void>();
  const windowEvents = new Map<string, () => void>();
  const timers = new Map<number, () => void>();
  const frames = new Map<number, () => void>();
  let sequence = 0;
  const input = {
    tagName: 'INPUT',
    disabled: false,
    matches: () => true,
    closest: () => null,
    get value(): never {
      return assert.fail('observer must never inspect input data');
    },
    focus(): never {
      assert.fail('observer must never change focus');
    },
  };
  const canvas = { nextElementSibling: input, matches: () => false };
  const doc = {
    readyState: 'complete',
    activeElement: input,
    getElementById: (id: string) => {
      assert.equal(id, 'deckcraft_canvas');
      return canvas;
    },
    querySelectorAll: (selector: string) => {
      assert.equal(selector, 'input');
      return Array(12);
    },
    hasFocus: () => true,
    addEventListener: (name: string, callback: () => void) => documentEvents.set(name, callback),
    removeEventListener: (name: string) => documentEvents.delete(name),
    get URL(): never {
      return assert.fail('observer must never inspect URLs');
    },
  };
  const window = {
    top: {},
    webkit: {
      messageHandlers: { ReactNativeWebView: { postMessage: (data: string) => sent.push(data) } },
    },
    addEventListener: (name: string, callback: () => void) => windowEvents.set(name, callback),
  };
  runInNewContext(deckcraftFocusQaScript(session), {
    window,
    document: doc,
    setTimeout: (callback: () => void, delay: number) => {
      assert.equal(delay, 500);
      const id = ++sequence;
      timers.set(id, callback);
      return id;
    },
    clearTimeout: (id: number) => timers.delete(id),
    requestAnimationFrame: (callback: () => void) => {
      const id = ++sequence;
      frames.set(id, callback);
      return id;
    },
    cancelAnimationFrame: (id: number) => frames.delete(id),
  });
  assert.equal(decodeDeckcraftFocusQa(sent[0]!, session)?.inputCount, 8);
  assert.equal(decodeDeckcraftFocusQa(sent[0]!, session)?.inputFocused, true);
  documentEvents.get('pointerup')!();
  assert.equal(decodeDeckcraftFocusQa(sent.at(-1)!, session)?.event, 'pointerup');
  for (const callback of timers.values()) callback();
  for (const callback of frames.values()) callback();
  assert.equal(decodeDeckcraftFocusQa(sent.at(-1)!, session)?.event, 'settled');
  const before = sent.length;
  windowEvents.get('pagehide')!();
  assert.equal(documentEvents.size, 0);
  assert.equal(timers.size, 0);
  assert.equal(frames.size, 0);
  assert.equal(sent.length, before);
});
