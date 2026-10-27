import assert from 'node:assert/strict';
import { test } from 'node:test';
import { MessageTextSelectionSession } from '../components/message-text-selection.ts';
import { MessageContextPresentation } from '../components/message-context-presentation.ts';
const deferred = <T>() => {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => {
    resolve = done;
  });
  return { promise, resolve };
};
const config = {
  actions: [{ id: 'select' as const, label: 'Select text' }],
  reactions: [],
  theme: {
    background: '#ffffff',
    highlight: '#eeeeee',
    text: '#000000',
    secondary: '#888888',
    separator: '#dddddd',
    destructive: '#ff0000',
    dim: '#00000088',
    isDark: false,
  },
  reducedMotion: true,
  incoming: true,
};

test('same context stays extracted through review and only returns Quote after actual restoration barrier', async () => {
  const review = deferred<string>(),
    restored = deferred<void>(),
    entered = deferred<void>(),
    result = deferred<string | null>();
  const events: string[] = [];
  const native = { presentContext: async () => result.promise, dismissContext: async () => {} };
  const owner = new MessageContextPresentation({
    request: {
      binding: 'binding',
      gestureId: 'gesture',
      kind: 'hold',
      current: () => true,
      native: native as any,
    },
    current: () => true,
    id: () => 'context',
    selected: (value) => events.push(value),
  });
  const opened = owner.open(config);
  const session = new MessageTextSelectionSession(async (id, payload) => {
    assert.equal(id, 'context');
    events.push('finish-requested');
    entered.resolve();
    await restored.promise;
    session.phase('gesture', 'context', false);
    result.resolve('select:' + payload);
  });
  session
    .binding('gesture', () => true)
    .bind({ started: () => events.push('selecting'), review: async () => review.promise });
  session.phase('gesture', 'context', true);
  assert.equal(session.getSnapshot().active, true);
  assert.deepEqual(events, ['selecting']);
  const quoted = session.quote('selected', 'mapped-spans');
  assert.equal(session.getSnapshot().pending, true);
  assert.deepEqual(events, ['selecting']);
  review.resolve('{"canonical":true}');
  await entered.promise;
  assert.deepEqual(events, ['selecting', 'finish-requested']);
  restored.resolve();
  await quoted;
  await opened;
  assert.deepEqual(events, ['selecting', 'finish-requested', 'select:{"canonical":true}']);
  assert.equal(session.getSnapshot().active, false);
});
test('native cancel during read-only review invalidates late result without finish or draft changes', async () => {
  const review = deferred<string>();
  let finishes = 0;
  const session = new MessageTextSelectionSession(async () => {
    finishes++;
  });
  session.binding('g', () => true).bind({ started() {}, review: () => review.promise });
  session.phase('g', 'c', true);
  const quoted = session.quote('text', 'spans');
  session.phase('g', 'c', false);
  review.resolve('{}');
  await quoted;
  assert.equal(finishes, 0);
  assert.deepEqual(session.getSnapshot(), { active: false, pending: false, error: false });
});
test('unsupported provenance or Core refusal keeps original selection surface active without finishing', async () => {
  let reads = 0,
    finishes = 0;
  const session = new MessageTextSelectionSession(async () => {
    finishes++;
  });
  session
    .binding('g', () => true)
    .bind({
      started() {},
      review: async () => {
        reads++;
        throw new Error('unsupported');
      },
    });
  session.phase('g', 'c', true);
  await session.quote('text');
  assert.equal(reads, 0);
  assert.equal(session.getSnapshot().error, true);
  await session.quote('text', 'spans');
  assert.equal(reads, 1);
  assert.equal(finishes, 0);
  assert.equal(session.getSnapshot().active, true);
});
test('foreign phase, old cleanup, owner loss and duplicate Quote cannot reuse selection authority', async () => {
  let current = true,
    reads = 0,
    finishes = 0;
  const read = deferred<string>();
  const session = new MessageTextSelectionSession(async () => {
    finishes++;
  });
  const old = session.binding('old', () => true).bind({ started() {}, review: async () => '' });
  session
    .binding('new', () => current)
    .bind({
      started() {},
      review: async () => {
        reads++;
        return read.promise;
      },
    });
  old();
  session.phase('old', 'foreign', true);
  assert.equal(session.getSnapshot().active, false);
  session.phase('new', 'context', true);
  const one = session.quote('text', 'spans');
  await session.quote('text', 'spans');
  assert.equal(reads, 1);
  session.phase('old', 'foreign', false);
  assert.equal(session.getSnapshot().active, true);
  current = false;
  read.resolve('{}');
  await one;
  assert.equal(finishes, 0);
});
