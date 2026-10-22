import assert from 'node:assert/strict';
import { test } from 'node:test';
import type { Element } from './reorderable-cards.fixtures.ts';
import { fixture, nativeDetectorFrame, tick } from './reorderable-cards.fixtures.ts';

test('one lift starts all cards editing with X and Pin on every card, and cover taps never launch', () => {
  const f = fixture();
  assert.ok(f.cards().every((card) => card.props.editActions === undefined));
  f.activate();
  f.release(true);
  f.render();
  assert.equal(f.editing, true);
  assert.ok(f.cards().every((card) => !!card.props.editActions));
  assert.ok(
    f.geometry().cards.every((card) => card.controlsVisible === true && card.editing === true),
  );
  (f.cards()[1]!.props.onPress as () => void)();
  f.render();
  assert.ok(f.cards().every((card) => !!card.props.editActions));
  assert.equal(f.opens, 0);
});

test('stale editing actions cannot execute after owner/layout, membership or lifecycle retirement', async () => {
  for (const boundary of [
    'context',
    'geometry',
    'membership',
    'blur',
    'background',
    'Done',
    'unmount',
  ]) {
    const f = fixture();
    f.activate();
    f.release(true);
    f.render();
    const actions = f.cards()[1]!.props.editActions as Element;
    if (boundary === 'context') {
      Object.assign(f.props, { contextKey: 'new-owner-layout' });
      f.render();
    } else if (boundary === 'geometry') {
      Object.assign(f.props, { horizontal: false, columns: 2 });
      f.render();
    } else if (boundary === 'membership') {
      f.props.items = f.props.items.filter((item) => item.id !== 'b');
      f.render();
    } else if (boundary === 'blur') {
      f.root.blur();
      f.root.focus();
    } else if (boundary === 'background') {
      f.state('background');
      f.state('active');
    } else if (boundary === 'Done') f.done();
    else f.root.unmount();
    (actions.props.onRemove as () => void)();
    (actions.props.onTogglePin as () => void)();
    await tick();
    assert.deepEqual(f.actions, [], boundary);
    assert.deepEqual(f.writes, [], boundary);
  }
});

test('an action on another card retires native callbacks, serializes all card taps and preserves a fresh tap after Done', async () => {
  const f = fixture();
  let finish!: () => void;
  f.hold(
    new Promise<void>((resolve) => {
      finish = resolve;
    }),
  );
  f.activate();
  f.release(true);
  f.render();
  const actions = f.cards()[1]!.props.editActions as Element;
  const other = f.cards()[2]!.props.editActions as Element;
  (actions.props.onTogglePin as () => void)();
  (other.props.onRemove as () => void)();
  (actions.props.onTogglePin as () => void)();
  f.render();
  assert.deepEqual(f.actions, ['pin:b']);
  assert.ok(f.cards().every((card) => (card.props.editActions as Element).props.disabled === true));
  f.activate();
  f.move(325);
  f.release();
  assert.deepEqual(f.writes, []);
  finish();
  await tick();
  f.render();
  f.done();
  (f.card().props.onPress as () => void)();
  assert.equal(f.opens, 0);
  f.child.gesture('onTouchesDown');
  (f.card().props.onPress as () => void)();
  assert.equal(f.opens, 1);
});

test('cover taps while editing do not invalidate controls on other current cards or open an App', async () => {
  const f = fixture();
  f.activate();
  f.release(true);
  f.render();
  const actions = f.cards()[2]!.props.editActions as Element;
  (f.cards()[1]!.props.onPress as () => void)();
  (f.card().props.onAccessibilityTap as () => void)();
  f.render();
  assert.ok(f.cards().every((card) => !!card.props.editActions));
  assert.ok(f.geometry().cards.every((card) => card.controlsVisible === true));
  (actions.props.onRemove as () => void)();
  await tick();
  assert.deepEqual(f.actions, ['remove:c']);
  assert.equal(f.opens, 0);
});

test('every editing cards44pt X/Pin hits are excluded before native long-press recognition, while every cover still lifts', () => {
  const f = fixture();
  assert.equal(f.child.holdAt(34, 34), true, 'ordinary cover lifts before editing');
  f.render();
  f.release(true);
  f.render();
  for (const id of ['a', 'b', 'c']) {
    const card = f.nativeCard(id);
    const before = f.haptics;
    for (const x of [12, 34, 55, 104, 126, 147])
      for (const y of [12, 34, 55])
        assert.equal(
          card.holdAt(x, y),
          false,
          `${id}: holding either control cannot enter native pan`,
        );
    assert.equal(f.haptics, before);
    assert.deepEqual(f.writes, []);
    assert.equal(card.holdAt(80, 80), true, `${id}: cover below controls still lifts`);
    assert.ok(f.haptics > before);
    card.gesture('onDeactivate', { canceled: true });
    card.gesture('onFinalize');
    card.unmount();
  }
});

test('nonediting first frames and rehydrated positions never spring, while intentional editing neighbors still animate', () => {
  const f = fixture();
  const read = (card: ReturnType<typeof f.nativeCard>) => {
    const tree = card.render();
    const animated = tree.type === 'AnimatedView' ? tree : (tree.props.children as Element);
    return (animated.props.style as { read?: () => unknown }[]).find((value) => value.read)!
      .read!() as { transform: Record<string, unknown>[] };
  };
  assert.equal(f.style().transform[0]!.translateX, 0);
  const initialB = f.nativeCard('b');
  assert.equal(read(initialB).transform[0]!.translateX, 170);
  assert.deepEqual(f.springs, [], 'first render is already at the final positions and scale');
  initialB.unmount();
  f.props.items = [...f.props.items].reverse();
  f.render();
  const restoredA = f.nativeCard('a');
  assert.equal(read(restoredA).transform[0]!.translateX, 340);
  assert.deepEqual(f.springs, [], 'hydrated preference order never flies in from zero');
  restoredA.unmount();
  f.activate();
  f.move(171);
  const neighbor = f.nativeCard('b');
  f.springs.length = 0;
  assert.equal(read(neighbor).transform[0]!.translateX, 0);
  assert.deepEqual(f.springs, [0, 0, 1], 'an intentional edit animates the displaced neighbor');
  f.release(true);
  f.springs.length = 0;
  assert.equal(read(neighbor).transform[0]!.translateX, 170);
  assert.deepEqual(
    f.springs,
    [170, 0, 1],
    'a canceled edit returns the neighbor with the existing spring',
  );
  neighbor.unmount();
  const reduced = fixture(true);
  reduced.activate();
  reduced.style();
  assert.deepEqual(reduced.springs, [], 'Reduce Motion remains direct even during editing');
});
test('native editing hit frames follow every translated rail and grid card while controls remain excluded', () => {
  for (const horizontal of [true, false]) {
    const f = fixture();
    Object.assign(f.props, { horizontal, columns: horizontal ? 3 : 2 });
    f.render();
    f.activate();
    f.release(true);
    f.render();
    for (const [index, id] of ['a', 'b', 'c'].entries()) {
      const native = f.nativeCard(id);
      const frame = nativeDetectorFrame(native.render());
      const expected = {
        x: horizontal ? index * 170 : (index % 2) * 170,
        y: horizontal ? 0 : Math.floor(index / 2) * 170,
        width: 160,
        height: 160,
      };
      assert.deepEqual(
        frame,
        expected,
        `${horizontal ? 'rail' : 'grid'} ${id}: detector inherits the visible card position`,
      );
      // UIKit's first native hitSlop gate must admit the painted cover center.
      const paintedX = expected.x + 80,
        paintedY = expected.y + 96;
      const localX = paintedX - frame.x,
        localY = paintedY - frame.y;
      assert.equal(
        native.holdAt(localX, localY),
        true,
        `${id}: visible cover reaches native activation`,
      );
      native.gesture('onDeactivate', { canceled: true });
      native.gesture('onFinalize');
      assert.equal(native.holdAt(34, 34), false, `${id}: 44pt control touch never starts pan`);
    }
    assert.deepEqual(f.writes, []);
    assert.equal(f.opens, 0);
  }
});

test('editing outline is painted above the cover inside its rounded160pt frame and shares the jiggle transform', () => {
  const f = fixture();
  f.activate();
  f.release(true);
  f.render();
  const native = f.nativeCard('a');
  const outer = native.render();
  const detector = outer.props.children as Element;
  const frame = detector.props.children as Element;
  const children = frame.props.children as Element[];
  assert.equal(children.length, 2);
  const outline = children[1]!;
  assert.equal(outline.props.pointerEvents, 'none');
  const style = outline.props.style as Record<string, unknown>;
  assert.equal(style.inset, 0);
  assert.equal(style.borderRadius, 24);
  assert.equal(style.borderCurve, 'continuous');
  assert.equal(style.borderWidth, 1);
  assert.equal((frame.props.style as Record<string, unknown>).width, 160);
  assert.equal((frame.props.style as Record<string, unknown>).height, 160);
  assert.equal(style.transform, undefined, 'Outline inherits the same complete card transform');
});

test('a header controller that refuses draft admission retires the native hold without editing or writes', () => {
  const f = fixture();
  f.props.onEditingChange = () => false;
  f.render();
  f.activate();
  f.release();
  f.render();
  assert.equal(f.editing, false);
  assert.equal(f.card().props.editing, false);
  assert.equal(f.opens, 0);
  assert.deepEqual(f.writes, []);
});
