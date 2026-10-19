import assert from 'node:assert/strict';
import { test } from 'node:test';
import { moveHomeOrder } from '../home/order.ts';

test('native moves rebase destinations before source removal, including the final position', () => {
  const ids = ['a', 'b', 'c', 'd'];
  assert.deepEqual(moveHomeOrder(ids, [0], 3), ['b', 'c', 'a', 'd']);
  assert.deepEqual(moveHomeOrder(ids, [0], 4), ['b', 'c', 'd', 'a']);
  assert.deepEqual(moveHomeOrder(ids, [2], 0), ['c', 'a', 'b', 'd']);
  assert.deepEqual(moveHomeOrder(ids, [1], 2), ids);
  assert.deepEqual(ids, ['a', 'b', 'c', 'd']);
});

test('multi-row moves keep source order and subtract every source preceding the destination', () => {
  const ids = ['a', 'b', 'c', 'd', 'e', 'f'];
  assert.deepEqual(moveHomeOrder(ids, [4, 1], 5), ['a', 'c', 'd', 'b', 'e', 'f']);
  assert.deepEqual(moveHomeOrder(ids, [1, 4], 2), ['a', 'b', 'e', 'c', 'd', 'f']);
  assert.deepEqual(moveHomeOrder(ids, [1, 2], 2), ids);
  assert.deepEqual(moveHomeOrder(ids, [0, 1, 2, 3, 4, 5], 6), ids);
});

test('invalid, duplicate and outdated native indices cannot produce a replacement order', () => {
  const ids = ['a', 'b', 'c'];
  for (const sources of [[], [-1], [3], [0.5], [NaN], [0, 0], [0, Infinity]])
    assert.equal(moveHomeOrder(ids, sources, 0), null);
  for (const destination of [-1, 4, 1.5, NaN, Infinity])
    assert.equal(moveHomeOrder(ids, [0], destination), null);
  assert.equal(moveHomeOrder(['a', 'a'], [0], 2), null);
  assert.equal(moveHomeOrder([], [0], 0), null);
});
