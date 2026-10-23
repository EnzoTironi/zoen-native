import assert from 'node:assert/strict';
import { test } from 'node:test';
import { fixture, tick } from './reorderable-cards.fixtures.ts';

test('queued native begin and drop remain valid in the normal deferred journey', async () => {
  const f = fixture(false, true);
  f.activate();
  f.move(325);
  f.release();
  assert.equal(f.editing, false);
  assert.equal(f.writes.length, 0);
  assert.ok(f.scheduledCount >= 2);
  f.flush();
  await tick();
  f.render();
  assert.equal(f.editing, true);
  assert.deepEqual(f.writes, [{ next: ['b', 'c', 'a'], expected: ['a', 'b', 'c'] }]);
});
test('queued native begin cannot reopen editing after blur/refocus or background/resume', () => {
  for (const boundary of ['blur', 'background']) {
    const f = fixture(false, true);
    f.activate();
    assert.equal(f.editing, false);
    if (boundary === 'blur') {
      f.root.blur();
      f.root.focus();
    } else {
      f.state('background');
      f.state('active');
    }
    f.flush();
    f.render();
    assert.equal(f.editing, false, boundary);
    assert.deepEqual(f.announcements, [], boundary);
    assert.equal(f.writes.length, 0, boundary);
  }
});
test('queued native begin is invalidated by same-ID geometry or full-order context changes', () => {
  for (const boundary of ['geometry', 'context']) {
    const f = fixture(false, true);
    f.activate();
    if (boundary === 'geometry') Object.assign(f.props, { horizontal: false, columns: 2 });
    else Object.assign(f.props, { contextKey: 'changed-full-order' });
    f.render();
    f.flush();
    f.render();
    assert.equal(f.editing, false, boundary);
    assert.equal(f.writes.length, 0, boundary);
  }
});
test('queued native drop cannot persist after same-ID geometry or full-order context changes', async () => {
  for (const boundary of ['geometry', 'context']) {
    const f = fixture(false, true);
    f.activate();
    f.flush();
    f.render();
    assert.equal(f.editing, true);
    f.move(325);
    f.release();
    assert.equal(f.writes.length, 0);
    if (boundary === 'geometry') Object.assign(f.props, { horizontal: false, columns: 2 });
    else Object.assign(f.props, { contextKey: 'changed-full-order' });
    f.render();
    f.flush();
    await tick();
    f.render();
    assert.equal(f.writes.length, 0, boundary);
    assert.equal(f.style().transform[0]!.translateX, 0, boundary);
  }
});
test('a pending native edit admission also suppresses a new physical tap before RN receives begin', () => {
  const f = fixture(false, true);
  f.activate();
  f.release(true);
  f.child.gesture('onTouchesDown');
  (f.card().props.onPress as () => void)();
  (f.card().props.onAccessibilityTap as () => void)();
  assert.equal(f.opens, 0);
  f.flush();
  f.render();
  assert.equal(f.editing, true);
});
test('a newer native lift supersedes queued callbacks from the previous activation', async () => {
  const f = fixture(false, true);
  f.activate();
  f.move(325);
  f.release();
  f.activate();
  f.release(true);
  f.flush();
  await tick();
  f.render();
  assert.equal(f.editing, true);
  assert.equal(f.writes.length, 0);
  assert.deepEqual(f.announcements, ['home.rearranging']);
});
test('retirement never reopens editing when an already-admitted preference write later succeeds', async () => {
  let resolve!: () => void;
  const f = fixture(false, true);
  f.hold(
    new Promise((yes) => {
      resolve = () => yes(undefined);
    }),
  );
  f.activate();
  f.flush();
  f.render();
  f.move(325);
  f.release();
  f.flush();
  assert.equal(f.writes.length, 1);
  f.root.blur();
  f.root.focus();
  f.render();
  resolve();
  await tick();
  f.render();
  assert.equal(f.editing, false);
  assert.equal(f.writes.length, 1); // An admitted durable write is not canceled or rolled back here.
});
