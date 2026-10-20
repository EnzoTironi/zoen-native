import assert from 'node:assert/strict';
import { test } from 'node:test';
import {
  CardReorderSession,
  cardDropIndex,
  cardEdgeVelocity,
  mergeCardSubset,
  moveCard,
} from './card-reorder.ts';
function deferred() {
  let resolve!: () => void;
  let reject!: (error: Error) => void;
  const promise = new Promise<void>((yes, no) => {
    resolve = yes;
    reject = no;
  });
  return { promise, resolve, reject };
}
test('a drop has one owner-authorized write, preserves expected order, and blocks a second drop until acknowledgment', async () => {
  const gate = deferred();
  const writes: unknown[] = [];
  const session = new CardReorderSession(['a', 'b', 'c'], async (next, expected) => {
    writes.push({ next, expected });
    await gate.promise;
  });
  assert.equal(await session.drop(['b', 'a', 'c'], ['a', 'b', 'c']), false);
  session.edit();
  const first = session.drop(['b', 'a', 'c'], ['a', 'b', 'c']);
  assert.equal(session.getSnapshot().pending, true);
  assert.deepEqual(session.getSnapshot().order, ['b', 'a', 'c']);
  assert.equal(await session.drop(['c', 'b', 'a'], ['a', 'b', 'c']), false);
  assert.deepEqual(writes, [{ next: ['b', 'a', 'c'], expected: ['a', 'b', 'c'] }]);
  session.update(['b', 'a', 'c']);
  gate.resolve();
  assert.equal(await first, true);
  assert.equal(session.getSnapshot().pending, false);
});
test('denied or failed persistence rolls the preview back to committed order', async () => {
  const gate = deferred();
  const session = new CardReorderSession(['a', 'b'], () => gate.promise);
  session.edit();
  const drop = session.drop(['b', 'a'], ['a', 'b']);
  gate.reject(new Error('access denied'));
  assert.equal(await drop, false);
  assert.deepEqual(session.getSnapshot(), { order: ['a', 'b'], editing: true, pending: false });
});
test('canonical reorder or retirement defeats late acknowledgement without reopening edit mode', async () => {
  for (const retire of [false, true]) {
    const gate = deferred();
    const session = new CardReorderSession(['a', 'b', 'c'], () => gate.promise);
    session.edit();
    const drop = session.drop(['b', 'a', 'c'], ['a', 'b', 'c']);
    if (retire) session.retire();
    session.update(['c', 'a', 'b']);
    gate.resolve();
    assert.equal(await drop, false);
    assert.deepEqual(session.getSnapshot().order, ['c', 'a', 'b']);
    assert.equal(session.getSnapshot().editing, !retire);
  }
});
test('stale IDs, incomplete membership, duplicates and no-op drops never write', async () => {
  let writes = 0;
  const session = new CardReorderSession(['a', 'b'], async () => {
    writes++;
  });
  session.edit();
  for (const [next, expected] of [
    [
      ['b', 'a'],
      ['b', 'a'],
    ],
    [['b'], ['a', 'b']],
    [
      ['b', 'b'],
      ['a', 'b'],
    ],
    [
      ['c', 'a'],
      ['a', 'b'],
    ],
    [
      ['a', 'b'],
      ['a', 'b'],
    ],
  ])
    assert.equal(await session.drop(next!, expected!), false);
  assert.equal(writes, 0);
});
test('available App order replaces only its slots, preserving favorite positions', () => {
  assert.deepEqual(mergeCardSubset(['p1', 'a', 'p2', 'b', 'c'], ['c', 'a', 'b']), [
    'p1',
    'c',
    'p2',
    'a',
    'b',
  ]);
  assert.deepEqual(moveCard(['a', 'b', 'c'], 'a', 2), ['b', 'c', 'a']);
});
test('drop geometry clamps grid and rail targets, and edge scroll is bounded and idle in the center', () => {
  assert.equal(cardDropIndex(170, 170, 5, 2, 166, 166), 3);
  assert.equal(cardDropIndex(900, 900, 5, 2, 166, 166), 4);
  assert.equal(cardDropIndex(-500, -500, 5, 2, 166, 166), 0);
  assert.equal(cardDropIndex(500, 0, 5, 5, 166, 166), 3);
  assert.equal(cardEdgeVelocity(200, 0, 400), 0);
  assert.equal(cardEdgeVelocity(-50, 0, 400), -260);
  assert.equal(cardEdgeVelocity(450, 0, 400), 260);
});
