import test from 'node:test';
import assert from 'node:assert/strict';
import { configureDisplayClock, displayNow, displayTime } from './display-clock.ts';
test('native display clock consumes fixed time and UTC, without replacing production wall time', t => {
  t.after(() => configureDisplayClock());
  const fixed = Date.parse('2026-01-15T12:00:00Z');
  configureDisplayClock({ now: fixed, timeZone: 'UTC' });
  assert.equal(displayNow(), fixed); assert.match(displayTime(fixed), /12:00/);
  configureDisplayClock(); assert.ok(Math.abs(displayNow() - Date.now()) < 1000);
});
