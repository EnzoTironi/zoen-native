import { test } from 'node:test';
import assert from 'node:assert/strict';
import { voiceHold } from './voice-hold.ts';
function setup() { const calls: string[] = []; return { calls, hold: voiceHold(200, 200, { send: () => calls.push('send'), cancel: () => calls.push('cancel') }) }; }
test('hold release sends exactly once', () => { const {hold,calls}=setup(); hold.release(); hold.release(); assert.deepEqual(calls,['send']); });
test('native left threshold cancels and never sends', () => { const {hold,calls}=setup(); assert.equal(hold.move(90,200),'holding'); assert.equal(hold.move(89,200),'ended'); hold.release(); assert.deepEqual(calls,['cancel']); });
test('native up threshold locks and release keeps recording', () => { const {hold,calls}=setup(); assert.equal(hold.move(200,120),'holding'); assert.equal(hold.move(200,119),'locked'); hold.release(); assert.deepEqual(calls,[]); hold.cancel(); assert.deepEqual(calls,['cancel']); });
test('cancellation during pending permission never sends', () => { const {hold,calls}=setup(); hold.cancel(); hold.release(); assert.deepEqual(calls,['cancel']); });
test('cancel wins over lock for diagonal gesture', () => { const {hold,calls}=setup(); assert.equal(hold.move(0,0),'ended'); hold.release(); assert.deepEqual(calls,['cancel']); });
