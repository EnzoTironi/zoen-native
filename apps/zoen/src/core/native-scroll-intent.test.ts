import assert from 'node:assert/strict';
import { test } from 'node:test';
import { nativeScrollIntentFixture } from './native-scroll-intent.fixtures.ts';

test('restoring a draft can issue an equal-valued native scroll after search moved the viewport', () => {
  const f = nativeScrollIntentFixture();
  assert.equal((f.props().contentOffset as { y: number }).y, 376);
  f.keyboard(320);
  assert.equal((f.props().contentOffset as { y: number }).y, 696);
  f.scroll.value = 696; // UIKit applied the keyboard command.
  f.extra.value = 52; // Search temporarily hides reply and attachment review.
  f.tick();
  assert.equal((f.props().contentOffset as { y: number }).y, 508);
  f.scroll.value = 508;
  f.scroll.value = 320; // The actual search controller separately revealed a result.
  assert.equal(
    f.props().contentOffset,
    undefined,
    'observing a search scroll cannot replay an old command',
  );
  f.extra.value = 240; // Close search and restore the exact retained draft.
  f.tick();
  assert.equal(
    (f.props().contentOffset as { y: number } | undefined)?.y,
    508,
    'a new 320+188 correction must reach UIKit even when the previous target was also 508',
  );
  assert.equal(f.props().contentOffset, undefined, 'the same intent is emitted only once');
});

test('inset-only updates after an external reveal do not replay the old keyboard command', () => {
  const f = nativeScrollIntentFixture();
  f.props();
  f.keyboard(320);
  const command = f.props().contentOffset as { y: number };
  assert.equal(command.y, 696);
  f.scroll.value = 120;
  f.tick();
  assert.equal(f.props().contentOffset, undefined);
  assert.equal(f.props().contentOffset, undefined);
});
