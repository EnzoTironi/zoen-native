import assert from 'node:assert/strict';
import test from 'node:test';
import {
  CONVERSATION_RAIL_GRIP,
  railDragExpansion,
  railSnapExpansion,
  railVisibleHeight,
} from './conversation-rail-motion.ts';

test('manual detents retain the reopen grip and combine with every keyboard frame', () => {
  const content = 180;
  assert.equal(railVisibleHeight(content, 1, 0, true), content + CONVERSATION_RAIL_GRIP);
  assert.equal(railVisibleHeight(content, 0, 0, true), CONVERSATION_RAIL_GRIP);
  for (const expansion of [0, 0.1, 0.5, 0.9, 1]) {
    const before = railVisibleHeight(content, expansion, 0, true);
    assert.equal(railVisibleHeight(content, expansion, 0.5, true), before / 2);
    assert.equal(railVisibleHeight(content, expansion, 1, true), 0);
  }
  assert.equal(railVisibleHeight(0, 1, 0, true), 0, 'an empty rail has no orphaned handle');
  assert.equal(
    railVisibleHeight(content, 0, 0, false),
    content,
    'noncollapsible notices remain readable',
  );
});

test('drag reverses from the actual interrupted position without scaling cards', () => {
  const halfway = railDragExpansion(1, -90, 180);
  assert.equal(halfway, 0.5);
  assert.equal(railDragExpansion(halfway, 45, 180), 0.75);
  assert.equal(railDragExpansion(1, -400, 180), 0);
  assert.equal(railDragExpansion(0, 400, 180), 1);
  assert.equal(railSnapExpansion(0.8, -800, 180), 0);
  assert.equal(railSnapExpansion(0.2, 800, 180), 1);
  assert.equal(railSnapExpansion(0.3, 0, 180), 0);
  assert.equal(railSnapExpansion(0.7, 0, 180), 1);
});

test('invalid geometry cannot reserve a negative or nonfinite obstruction', () => {
  for (const height of [0, -1, NaN, Infinity]) {
    assert.equal(railVisibleHeight(height, 0.5, 0.5, true), 0);
    assert.equal(railDragExpansion(0.5, 50, height), 0.5);
  }
  assert.equal(railVisibleHeight(180, 5, -2, true), 180 + CONVERSATION_RAIL_GRIP);
  assert.equal(railVisibleHeight(180, -5, 2, true), 0);
});
