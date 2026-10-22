import assert from 'node:assert/strict';
import { test } from 'node:test';
import { albumMetadata, fixture, swipe } from './message-interaction.fixtures.ts';

for (const delivery of ['Failed', 'Sent'] as const)
  test(`actual ${delivery} album metadata tap opens its own menu without an outer swipe`, () => {
    const actual = albumMetadata(delivery);
    const f = fixture(actual.interaction);
    f.begin();
    assert.equal(f.press(actual.action), 1);
    assert.deepEqual(actual.menus, [{ id: 'signed-album-first-message', mode: 'actions' }]);
    assert.deepEqual(f.states, ['fail']);
    f.move(-12);
    f.update(-100);
    f.end(-100);
    f.flush();
    assert.deepEqual(f.calls, []);
    assert.deepEqual(f.impacts, []);
  });

for (const policy of ['swipeEnabled', 'disabled'] as const)
  test(`a real pan retired by ${policy} cannot release into an album control`, () => {
    const f = fixture();
    f.begin();
    f.move(-10);
    f.update(-80);
    f.render(policy === 'disabled' ? { disabled: true } : { swipeEnabled: false });
    assert.equal(f.press(), 0, 'Policy change must not forgive a consumed physical touch');
    f.end(-80);
    f.flush();
    assert.deepEqual(f.calls, []);
    assert.deepEqual(f.impacts, []);
    f.render({ disabled: false, swipeEnabled: false });
    assert.equal(f.press(), 0, 'Re-enable does not itself authorize the old release');
    f.begin();
    assert.equal(f.press(), 1, 'Only a new passive-row touch authorizes a new press');
  });

test('passive album controls retain media-consumption, focus and background retirement fences', () => {
  const f = fixture({ swipeEnabled: false });
  assert.equal(f.press(), 0);
  f.begin();
  assert.equal(f.press(), 1);
  f.consume();
  assert.equal(f.press(), 0);
  f.begin();
  assert.equal(f.press(), 1);
  f.blur();
  f.focus();
  assert.equal(f.press(), 0);
  f.begin();
  assert.equal(f.press(), 1);
  f.app('inactive');
  f.app('active');
  assert.equal(f.press(), 0);
  f.begin();
  assert.equal(f.press(), 1);
  f.unmount();
  assert.equal(f.press(), 0);
});

test('queued active-row intent cannot borrow the new passive album touch after a policy change', () => {
  const f = fixture();
  swipe(f, -80);
  f.render({ swipeEnabled: false });
  f.begin();
  assert.equal(f.press(), 1);
  f.flush();
  assert.deepEqual(f.calls, []);
  assert.deepEqual(f.impacts, []);
});

test('old active native configuration cannot rearm a retired row or steal a new passive control touch', () => {
  const f = fixture();
  const old = f.pan;
  f.begin();
  f.move(-10);
  f.render({ swipeEnabled: false });
  const touch = {
    handlerTag: 9,
    numberOfTouches: 1,
    allTouches: [{ absoluteX: 90, absoluteY: 100 }],
  };
  old.onTouchesDown!(touch);
  assert.equal(f.press(), 0, 'A retired configuration must not rearm the old release');
  f.begin();
  old.onTouchesMove!({ ...touch, allTouches: [{ absoluteX: 10, absoluteY: 100 }] });
  old.onUpdate!({ translationX: -80 });
  old.onDeactivate!({ translationX: -80, canceled: false });
  old.onFinalize!({});
  assert.equal(f.press(), 1, 'The new passive touch remains independently pressable');
  f.flush();
  assert.deepEqual(f.calls, []);
  assert.deepEqual(f.impacts, []);
});
