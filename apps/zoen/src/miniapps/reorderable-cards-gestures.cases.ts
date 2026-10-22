import assert from 'node:assert/strict';
import { test } from 'node:test';
import { fixture, tick } from './reorderable-cards.fixtures.ts';

test('real compiled card gesture enters editing, suppresses trailing launch, and writes once on drop', async () => {
  const f = fixture();
  f.activate();
  assert.equal(f.editing, true);
  (f.card().props.onPress as () => void)();
  assert.equal(f.opens, 0);
  f.move(325);
  assert.equal(f.writes.length, 0);
  f.release();
  await tick();
  f.render();
  assert.deepEqual(f.writes, [{ next: ['b', 'c', 'a'], expected: ['a', 'b', 'c'] }]);
  assert.ok(f.haptics >= 2);
  assert.deepEqual(f.announcements, ['home.rearranging', 'home.movedPosition']);
});
test('compiled reorder bounds use the same fixed square and 170 pt stride in rails and grids', async () => {
  const f = fixture();
  assert.equal(f.geometry().container.width, 500);
  assert.equal(f.geometry().container.height, 160);
  for (const card of f.geometry().cards) {
    assert.equal(card.size, 160);
    assert.equal(card.strideX, 170);
    assert.equal(card.strideY, 170);
    assert.equal(card.onMeasure, undefined);
  }
  assert.equal(f.geometry().nativeFrame[0]!.width, 160);
  assert.equal(f.geometry().nativeFrame[0]!.height, 160);
  Object.assign(f.props, { horizontal: false, columns: 2 });
  f.render();
  assert.equal(f.geometry().container.width, 330);
  assert.equal(f.geometry().container.height, 330);
  assert.equal(f.geometry().container.alignSelf, 'center');
  const beforeHalf = fixture();
  beforeHalf.activate();
  beforeHalf.move(84);
  beforeHalf.release();
  await tick();
  assert.deepEqual(beforeHalf.writes, []);
  const afterHalf = fixture();
  afterHalf.activate();
  afterHalf.move(86);
  afterHalf.release();
  await tick();
  assert.deepEqual(afterHalf.writes, [{ next: ['b', 'a', 'c'], expected: ['a', 'b', 'c'] }]);
});
test('real compiled canceled drag restores geometry and never persists', async () => {
  const f = fixture();
  f.activate();
  f.move(325);
  f.release(true);
  await tick();
  assert.equal(f.writes.length, 0);
  assert.equal(f.style().transform[0]!.translateX, 0);
});
test('background or route retirement cancels gesture and late drop/press callbacks', async () => {
  for (const retire of ['background', 'blur', 'unmount']) {
    const f = fixture();
    f.activate();
    f.move(325);
    const press = f.card().props.onPress as () => void;
    if (retire === 'background') f.state('background');
    else if (retire === 'blur') f.root.blur();
    else f.root.unmount();
    f.release();
    press();
    await tick();
    assert.equal(f.writes.length, 0, retire);
    assert.equal(f.opens, 0, retire);
    assert.equal(f.editing, false, retire);
  }
});
test('changed larger order scope or layout cancels a held card before drop', async () => {
  for (const change of ['context', 'geometry', 'membership']) {
    const f = fixture();
    f.activate();
    f.move(325);
    if (change === 'context') Object.assign(f.props, { contextKey: 'new-full-Home-order' });
    else if (change === 'geometry') Object.assign(f.props, { horizontal: false, columns: 2 });
    else f.props.items = f.props.items.slice(0, 2);
    f.render();
    f.release();
    await tick();
    assert.equal(f.writes.length, 0, change);
  }
});
test('denied deferred save rolls back and retirement cannot reopen editing on late acknowledgment', async () => {
  for (const retire of [false, true]) {
    let reject!: (error: Error) => void;
    const f = fixture();
    f.hold(
      new Promise((_yes, no) => {
        reject = no;
      }),
    );
    f.activate();
    f.move(325);
    f.release();
    if (retire) f.state('inactive');
    reject(new Error('denied'));
    await tick();
    f.render();
    assert.equal(f.style().transform[0]!.translateX, 0);
    assert.equal(f.editing, !retire);
  }
});
test('Reduce Motion has no rotation/lift while VoiceOver reorder still persists exact order', async () => {
  const f = fixture(true);
  f.activate();
  assert.equal(f.style().transform[2]!.scale, 1);
  assert.equal(f.style().transform[3]!.rotateZ, '0deg');
  f.release(true);
  f.render();
  const actions = f.card().props.accessibilityActions as { name: string }[];
  assert.deepEqual(
    Array.from(actions, (action) => action.name),
    ['moveLater'],
  );
  (f.card().props.onAccessibilityAction as (event: unknown) => void)({
    nativeEvent: { actionName: 'moveLater' },
  });
  await tick();
  assert.deepEqual(f.writes, [{ next: ['b', 'a', 'c'], expected: ['a', 'b', 'c'] }]);
});
test('real frame callback scrolls only near an edge and clamps the rail extent', () => {
  const f = fixture();
  f.activate();
  f.move(0, 150);
  f.root.frame();
  assert.deepEqual(f.scrolls, []);
  f.move(0, 299);
  f.root.frame();
  assert.ok(f.scrolls[0]! > 0);
  f.scrollPort.offset.set(299);
  f.root.frame();
  assert.equal(f.scrollPort.offset.get(), 300);
  f.release(true);
  f.root.frame();
  assert.equal(f.scrollPort.offset.get(), 300);
});

test('Done ends the jiggle and preserves the next ordinary tap, while the drag tail cannot launch', () => {
  const f = fixture();
  f.activate();
  f.release(true);
  f.render();
  assert.notEqual(f.style().transform[3]!.rotateZ, '0deg');
  f.done();
  assert.equal(f.style().transform[3]!.rotateZ, '0deg');
  (f.card().props.onPress as () => void)();
  assert.equal(f.opens, 0);
  f.child.gesture('onTouchesDown');
  (f.card().props.onPress as () => void)();
  assert.equal(f.opens, 1);
});

test('VoiceOver activates an ordinary card without a physical pan-down and cannot launch while editing', () => {
  const f = fixture();
  (f.card().props.onAccessibilityTap as () => void)();
  assert.equal(f.opens, 1);
  f.activate();
  (f.card().props.onAccessibilityTap as () => void)();
  assert.equal(f.opens, 1);
});
