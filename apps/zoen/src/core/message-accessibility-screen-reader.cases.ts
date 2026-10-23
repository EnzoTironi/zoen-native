import assert from 'node:assert/strict';
import { test } from 'node:test';
import { drain, fixture } from './message-accessibility.fixtures.ts';

test('screen-reader on/off events update the mounted host and cleanup rejects late native events', async () => {
  const f = fixture();
  assert.equal(f.render().group.props.accessible, false);
  f.initialQuery.resolve(false);
  await drain();
  f.emit(true);
  assert.equal(f.render().group.props.accessible, true);
  f.emit(false);
  assert.equal(f.render().group.props.accessible, false);
  assert.equal(f.registrations(), 1);
  assert.equal(f.listeners(), 1);
  f.unmount();
  assert.equal(f.removals(), 1);
  assert.equal(f.listeners(), 0);
  const writes = f.writes();
  f.lateEvent(true);
  assert.equal(f.writes(), writes);
});

test('a late initial screen-reader query cannot overwrite a newer native on or off event', async () => {
  for (const enabled of [true, false]) {
    const f = fixture();
    f.render();
    assert.equal(f.lifecycle()[0], 'subscribe', 'Subscribe before querying initial native state');
    assert.equal(f.listeners(), 1, 'Subscribe before the asynchronous initial query settles');
    f.emit(enabled);
    assert.equal(f.render().group.props.accessible, enabled);
    f.initialQuery.resolve(!enabled);
    await drain();
    assert.equal(f.render().group.props.accessible, enabled);
    f.unmount();
  }
});

test('failed or detached initial queries cannot erase an observed state or update an unmounted row', async () => {
  const failed = fixture();
  failed.render();
  failed.emit(true);
  failed.initialQuery.reject(new Error('Synthetic AccessibilityInfo failure'));
  await drain();
  assert.equal(failed.render().group.props.accessible, true);
  failed.unmount();
  const detached = fixture();
  detached.render();
  detached.unmount();
  const writes = detached.writes();
  detached.initialQuery.resolve(true);
  await drain();
  assert.equal(detached.writes(), writes);
  assert.equal(detached.removals(), 1);
});
