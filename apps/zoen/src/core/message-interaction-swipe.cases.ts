import assert from 'node:assert/strict';
import { test } from 'node:test';
import type { Element } from './message-interaction.fixtures.ts';
import { elements, fixture, json, styles, swipe } from './message-interaction.fixtures.ts';

test('real compiled gesture sends one directional intent beyond incoming/own threshold and one heavy impact', () => {
  for (const mine of [false, true])
    for (const direction of [-1, 1]) {
      const f = fixture({ mine });
      const threshold = mine ? 60 : 45;
      f.begin();
      f.move(direction * 10);
      f.update(direction * threshold);
      f.update(direction * (threshold + 1));
      f.update(0);
      f.update(-direction * (threshold + 10));
      f.end(direction * (threshold + 1));
      assert.equal(f.calls.length, 0, 'Native→RN action remains scheduled');
      f.flush();
      assert.deepEqual(f.calls, [direction < 0 ? 'reply' : 'thread']);
      assert.deepEqual(f.impacts, ['heavy']);
      assert.deepEqual(f.states, ['activate']);
    }
});

test('strict boundary, canceled drag and velocity-only flick never send an action', () => {
  for (const mine of [false, true])
    for (const canceled of [false, true]) {
      const f = fixture({ mine });
      f.begin();
      f.move(-10);
      f.update(mine ? -60 : -45);
      f.end(canceled ? -100 : mine ? -60 : -45, canceled, -2000);
      f.flush();
      assert.deepEqual(f.calls, []);
    }
  const flick = fixture();
  flick.begin();
  flick.move(-10);
  flick.update(-36);
  flick.end(-36, false, -2000);
  flick.flush();
  assert.deepEqual(flick.calls, []);
});

for (const boundary of ['unmount', 'row', 'disable', 'focus', 'background', 'activation'] as const)
  test(`queued action and impact retire across ${boundary} before RN dispatch`, () => {
    const f = fixture();
    swipe(f, -80);
    assert.equal(f.pending.length, 2);
    if (boundary === 'unmount') f.unmount();
    else if (boundary === 'row') f.render({ messageId: 'm2' });
    else if (boundary === 'disable') {
      f.render({ disabled: true });
      f.render({ disabled: false });
    } else if (boundary === 'focus') {
      f.blur();
      f.focus();
    } else if (boundary === 'background') {
      f.app('inactive');
      f.app('active');
    } else f.begin();
    f.flush();
    assert.deepEqual(f.calls, []);
    assert.deepEqual(f.impacts, []);
  });

test('scheduled intent uses the latest committed handler without inventing canonical authority', () => {
  const f = fixture();
  swipe(f, -80);
  f.render({ onReply: () => f.calls.push('latest') });
  f.flush();
  assert.deepEqual(f.calls, ['latest']);
});

test('back edge, vertical and multiple pointers fail admission without consuming normal press', () => {
  const f = fixture();
  f.begin(23);
  assert.deepEqual(f.states, ['fail']);
  assert.equal(f.press(), 1);
  f.begin();
  f.move(0, 20);
  assert.equal(f.states.at(-1), 'fail');
  f.begin();
  f.move(10, 0, 90, 2);
  assert.equal(f.states.at(-1), 'fail');
});

test('message pan and nested media consumption suppress trailing physical press, next touch rearms', () => {
  const f = fixture();
  f.begin();
  assert.equal(f.press(), 1);
  f.consume();
  assert.equal(f.press(), 0);
  f.begin();
  f.move(-10);
  f.end(-20);
  assert.equal(f.press(), 0);
  f.begin();
  assert.equal(f.press(), 1);
  f.blur();
  f.focus();
  assert.equal(f.press(), 0);
});

test('reduced motion keeps reply/thread accessible while silencing impact and displacement', () => {
  const f = fixture({
    accessibilityLabel: 'Message body',
    onActions: () => f.calls.push('actions'),
    actionsAccessibilityLabel: 'Actions',
  });
  f.reduced(true);
  swipe(f, -80);
  f.flush();
  assert.deepEqual(f.calls, ['reply']);
  assert.deepEqual(f.impacts, []);
  assert.deepEqual(f.animation, []);
  f.voiceOver(true);
  const row = elements(f.tree).find((item) => item.props.accessible === true)!;
  const action = row.props.onAccessibilityAction as (event: unknown) => void;
  action({ nativeEvent: { actionName: 'thread' } });
  assert.deepEqual(f.calls, ['reply', 'thread']);
  const movement = styles(row).transform as { translateX: number }[];
  assert.equal(movement[0]!.translateX, 0);
});

test('indicator shares moving row coordinate space, source 33pt geometry and independent release fade/recoil', () => {
  for (const mine of [false, true]) {
    const f = fixture({ mine });
    f.begin();
    f.move(-10);
    f.update(-20);
    f.render();
    const detector = elements(f.tree).find((item) => item.type === 'Detector')!;
    const moving = detector.props.children as Element;
    assert.equal(styles(moving).width, '100%');
    assert.equal((styles(moving).transform as { translateX: number }[])[0]!.translateX, -20);
    const reply = f.indicator('reply');
    const style = styles(reply);
    assert.equal(style.width, 33);
    assert.equal(style.height, 33);
    assert.equal(style.right, mine ? -43 : -9);
    const thread = f.indicator('thread');
    assert.equal(styles(thread).left, -43);
    f.end(-20);
    const recoil = f.animation.filter(
      (item) => (item.options as { duration: number }).duration === 300,
    );
    assert.equal(recoil.length, 2);
    assert.deepEqual(json(recoil.map((item) => item.options)), [
      { duration: 300, easing: [0.38, 0.7, 0.125, 1] },
      { duration: 300, easing: [0.38, 0.7, 0.125, 1] },
    ]);
  }
});

// A retained native configuration cannot acquire the new row epoch after recycling.
test('old native worklet entry and finalization cannot activate or reset a recycled row', () => {
  const f = fixture();
  const previous = f.pan;
  f.render({ messageId: 'm2' });
  previous.onTouchesDown!({
    numberOfTouches: 1,
    handlerTag: 9,
    allTouches: [{ absoluteX: 90, absoluteY: 100 }],
  });
  previous.onTouchesMove!({
    numberOfTouches: 1,
    handlerTag: 9,
    allTouches: [{ absoluteX: 70, absoluteY: 100 }],
  });
  previous.onUpdate!({ translationX: -80 });
  previous.onDeactivate!({ translationX: -80, canceled: false });
  previous.onFinalize!({});
  f.flush();
  assert.deepEqual(f.calls, []);
  assert.deepEqual(f.impacts, []);
  assert.deepEqual(f.states, ['fail']);
  swipe(f, -80);
  f.flush();
  assert.deepEqual(f.calls, ['reply']);
});

for (const boundary of ['row', 'disable', 'focus', 'background', 'activation'] as const)
  test(`old scheduled finish cannot borrow a fresh completed gesture after ${boundary}`, () => {
    const f = fixture();
    swipe(f, -80);
    if (boundary === 'row') f.render({ messageId: 'm2' });
    else if (boundary === 'disable') {
      f.render({ disabled: true });
      f.render({ disabled: false });
    } else if (boundary === 'focus') {
      f.blur();
      f.focus();
    } else if (boundary === 'background') {
      f.app('inactive');
      f.app('active');
    }
    swipe(f, 80);
    f.flush();
    assert.deepEqual(f.calls, ['thread']);
    assert.deepEqual(f.impacts, ['heavy']);
  });

test('changing reduced motion before queued impact quiets it without rearming the gesture', () => {
  const f = fixture();
  f.begin();
  f.move(-10);
  f.update(-60);
  f.reduced(true);
  f.flush();
  assert.deepEqual(f.impacts, []);
  f.reduced(false);
  f.update(-80);
  f.end(-80);
  f.flush();
  assert.deepEqual(f.impacts, []);
  assert.deepEqual(f.calls, ['reply']);
});
