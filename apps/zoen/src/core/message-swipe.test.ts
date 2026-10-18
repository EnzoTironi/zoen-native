import test from 'node:test';
import assert from 'node:assert/strict';
import { messageSwipeIntent, messageSwipeOffset, messageSwipeAdmission } from './message-swipe.ts';

test('message gestures map signed threshold to reply/thread and refuse incidental movement', () => {
  assert.equal(messageSwipeIntent(-64, 0), 'reply');
  assert.equal(messageSwipeIntent(64, 0), 'thread');
  assert.equal(messageSwipeIntent(63, 0), null);
  assert.equal(messageSwipeIntent(-63, 0), null);
  assert.equal(messageSwipeIntent(20, 1200), null);
  assert.equal(messageSwipeIntent(-36, -900), 'reply');
  assert.equal(messageSwipeIntent(36, 900), 'thread');
  assert.equal(messageSwipeIntent(36, -900), null);
  assert.equal(messageSwipeIntent(Number.NaN, 0), null);
  assert.equal(messageSwipeIntent(64, Infinity), null);
});
test('message rubber band is continuous, symmetric and bounded for long drags', () => {
  assert.equal(messageSwipeOffset(0), 0);
  assert.equal(messageSwipeOffset(64), 57.6);
  assert.equal(messageSwipeOffset(-64), -57.6);
  assert.ok(messageSwipeOffset(65) > messageSwipeOffset(64));
  assert.ok(messageSwipeOffset(65) - messageSwipeOffset(64) < 1);
  assert.equal(messageSwipeOffset(10000), 104);
  assert.equal(messageSwipeOffset(-10000), -104);
});

test('system back edge, vertical movement and multitouch win before message pan activation', () => {
  assert.equal(messageSwipeAdmission(23, 80, 0), 'fail');
  assert.equal(messageSwipeAdmission(24, 13, 0), 'activate');
  assert.equal(messageSwipeAdmission(90, 80, 13), 'fail');
  assert.equal(messageSwipeAdmission(90, 12, 4), 'wait');
  assert.equal(messageSwipeAdmission(90, 14, 10), 'wait');
  assert.equal(messageSwipeAdmission(90, -24, 10), 'activate');
  assert.equal(messageSwipeAdmission(90, 24, 0, 2), 'fail');
  assert.equal(messageSwipeAdmission(NaN, 80, 0), 'fail');
});
