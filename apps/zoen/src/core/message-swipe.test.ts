import test from 'node:test';
import assert from 'node:assert/strict';
import {
  messageSwipeIntent,
  messageSwipeOffset,
  messageSwipeAdmission,
  messageSwipeFeedback,
  messageSwipeThreshold,
} from './message-swipe.ts';

test('incoming 45pt and own 60pt releases use a strict raw boundary in either supported direction', () => {
  for (const mine of [false, true]) {
    const threshold = mine ? 60 : 45;
    assert.equal(messageSwipeThreshold(mine), threshold);
    for (const sign of [-1, 1]) {
      assert.equal(messageSwipeIntent(sign * (threshold - 0.01), mine), null);
      assert.equal(messageSwipeIntent(sign * threshold, mine), null);
      assert.equal(
        messageSwipeIntent(sign * (threshold + 0.01), mine),
        sign < 0 ? 'reply' : 'thread',
      );
    }
    for (const value of [NaN, Infinity, -Infinity])
      assert.equal(messageSwipeIntent(value, mine), null);
  }
});

test('threshold feedback occurs once per gesture, including retreat and reversal', () => {
  for (const mine of [false, true]) {
    const threshold = messageSwipeThreshold(mine);
    let played = false;
    const ticks: number[] = [];
    for (const value of [-threshold, -threshold - 10, 0, -threshold, threshold, threshold + 20]) {
      const feedback = messageSwipeFeedback(played, value, false, mine);
      played = feedback.played;
      if (feedback.tick) ticks.push(value);
    }
    assert.deepEqual(ticks, [-threshold]);
    assert.equal(played, true);
    assert.deepEqual(messageSwipeFeedback(false, -threshold, false, mine), {
      played: true,
      tick: true,
    });
  }
});

test('invalid and reduced motion updates do not emit feedback or rearm a held gesture', () => {
  for (const value of [null, NaN, Infinity, -Infinity]) {
    assert.deepEqual(messageSwipeFeedback(false, value, false, false), {
      played: false,
      tick: false,
    });
    assert.deepEqual(messageSwipeFeedback(true, value, false, false), {
      played: true,
      tick: false,
    });
  }
  const quiet = messageSwipeFeedback(false, -45, true, false);
  assert.deepEqual(quiet, { played: true, tick: false });
  assert.deepEqual(messageSwipeFeedback(quiet.played, 60, false, false), {
    played: true,
    tick: false,
  });
});

test('rational rubber band is continuous, symmetric and approaches the source range', () => {
  for (const mine of [false, true]) {
    const threshold = messageSwipeThreshold(mine);
    assert.equal(messageSwipeOffset(0, mine), 0);
    assert.equal(messageSwipeOffset(threshold, mine), threshold);
    assert.equal(messageSwipeOffset(threshold - 1, mine), threshold - 1);
    assert.ok(messageSwipeOffset(threshold + 1, mine) > threshold);
    assert.ok(messageSwipeOffset(threshold + 1, mine) < threshold + 1);
    assert.equal(messageSwipeOffset(threshold + 250, mine), threshold + 50);
    for (const distance of [1, threshold, threshold + 250, 1e6]) {
      assert.equal(messageSwipeOffset(-distance, mine), -messageSwipeOffset(distance, mine));
      assert.ok(messageSwipeOffset(distance, mine) < threshold + 100);
    }
    for (const value of [NaN, Infinity, -Infinity])
      assert.equal(messageSwipeOffset(value, mine), 0);
  }
});

test('2pt horizontal/2-to-1 direction admission preserves vertical scrolling, multitouch and 24pt back edge', () => {
  assert.equal(messageSwipeAdmission(23.99, 80, 0), 'fail');
  assert.equal(messageSwipeAdmission(24, 2, 0), 'wait');
  assert.equal(messageSwipeAdmission(24, 2.01, 0), 'activate');
  assert.equal(messageSwipeAdmission(90, 12, 6), 'wait');
  assert.equal(messageSwipeAdmission(90, 12.01, 6), 'activate');
  assert.equal(messageSwipeAdmission(90, 6, 12.01), 'fail');
  assert.equal(messageSwipeAdmission(90, -24, 10), 'activate');
  assert.equal(messageSwipeAdmission(90, 24, 0, 2), 'fail');
  for (const values of [
    [NaN, 80, 0],
    [90, Infinity, 0],
    [90, 80, NaN],
  ])
    assert.equal(messageSwipeAdmission(values[0]!, values[1]!, values[2]!), 'fail');
});
