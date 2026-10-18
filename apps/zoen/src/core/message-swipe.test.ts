import test from 'node:test';
import assert from 'node:assert/strict';
import {
  messageSwipeIntent,
  messageSwipeOffset,
  messageSwipeAdmission,
  messageSwipeFeedback,
  type MessageSwipeIntent,
} from './message-swipe.ts';

test('threshold feedback rearms after retreat and tracks a reversal without repeated held ticks', () => {
  let armed: MessageSwipeIntent | null = null;
  const ticks: MessageSwipeIntent[] = [];
  for (const distance of [-63, -64, -80, -40, -64, 64, 80]) {
    const feedback = messageSwipeFeedback(armed, distance, false);
    armed = feedback.armed;
    if (feedback.tick && armed) ticks.push(armed);
  }
  assert.deepEqual(ticks, ['reply', 'reply', 'thread']);
  assert.equal(armed, 'thread');
});

test('cancellation and invalid movement disarm threshold feedback without a tick', () => {
  for (const distance of [null, NaN, Infinity, -Infinity]) {
    const cancelled = messageSwipeFeedback('reply', distance, false);
    assert.deepEqual(cancelled, { armed: null, tick: false });
    assert.deepEqual(messageSwipeFeedback(cancelled.armed, -64, false), {
      armed: 'reply',
      tick: true,
    });
  }
});

test('live reduced motion keeps directional state while quieting new threshold feedback', () => {
  const quiet = messageSwipeFeedback(null, -64, true);
  assert.deepEqual(quiet, { armed: 'reply', tick: false });
  assert.deepEqual(messageSwipeFeedback(quiet.armed, -80, false), {
    armed: 'reply',
    tick: false,
  });
  const retreated = messageSwipeFeedback(quiet.armed, -30, false);
  assert.deepEqual(messageSwipeFeedback(retreated.armed, 64, false), {
    armed: 'thread',
    tick: true,
  });
});

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
