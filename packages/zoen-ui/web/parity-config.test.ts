import test from 'node:test';
import assert from 'node:assert/strict';
import { PARITY_FIXTURE, PARITY_SCREENS, parityConfig } from './parity-config.ts';
const valid = { ...PARITY_FIXTURE, source_revision: 'a'.repeat(40), appearance: 'light', viewport: 'narrow', screen: 'chats' };
test('native parity applies only the exact supported deterministic contract', () => {
  for (const screen of PARITY_SCREENS) assert.equal(parityConfig({ ...valid, screen }).screen, screen);
  for (const key of Object.keys(PARITY_FIXTURE)) assert.throws(() => parityConfig({ ...valid, [key]: 'different' }));
  assert.throws(() => parityConfig({ ...valid, source_revision: 'HEAD' }));
  assert.equal(parityConfig({ ...valid, viewport: 'wide' }).viewport, 'wide');
  assert.throws(() => parityConfig({ ...valid, viewport: 'invented' }));
  assert.throws(() => parityConfig({ ...valid, screen: 'invented' }));
  assert.throws(() => parityConfig({ ...valid, appearance: 'auto' }));
});
