import assert from 'node:assert/strict';
import { test } from 'node:test';
import { nativeScrollIntentFixture } from './native-scroll-intent.fixtures.ts';

// QA644a's first close: a 91⅓pt search composer restores through two actual
// layouts, 264pt then 279⅓pt, before UIKit observes either scroll correction.
// These measurements exercise the installed SDK, not an application formula.
function restoringDraft() {
  const f = nativeScrollIntentFixture({
    initialExtra: 91 + 1 / 3,
    initialOffset: 103 + 1 / 3,
    viewportHeight: 874,
    contentHeight: 886,
    offset: 26,
  });
  f.props();
  f.keyboard(328);
  const command = f.props().contentOffset as { y: number };
  assert.ok(Math.abs(command.y - (405 + 1 / 3)) < 0.001);
  f.scroll.value = command.y; // The native search viewport is acknowledged.
  return f;
}

function close(f: ReturnType<typeof restoringDraft>, acknowledgeFirst = false) {
  f.extra.value = 264;
  f.tick();
  if (acknowledgeFirst) f.scroll.value = (f.props().contentOffset as { y: number }).y;
  f.extra.value = 279 + 1 / 3;
  f.tick();
  return f.props();
}

test('coalesced reply and attachment layouts preserve every unacknowledged padding delta', () => {
  const f = restoringDraft();
  const props = close(f);
  const target = (props.contentOffset as { y: number }).y;
  assert.ok(Math.abs(target - (593 + 1 / 3)) < 0.001, `actual command: ${target}`);
  assert.ok(Math.abs((props.contentInset as { bottom: number }).bottom - (581 + 1 / 3)) < 0.001);
  // RN Fabric applies the newest props when UI-thread updates coalesce. Its
  // native contentInset and offset are distinct; an inset ACK alone is insufficient.
  const composerTop = 874 - (props.contentInset as { bottom: number }).bottom;
  const lastMessageBottom = 886 - target - 12;
  assert.ok(Math.abs(composerTop - lastMessageBottom - 12) < 0.001);
  assert.equal(f.props().contentOffset, undefined, 'no blind replay after the fresh command');
});

test('separate native acknowledgements and coalesced layouts converge to the same target', () => {
  const coalesced = close(restoringDraft());
  const separate = close(restoringDraft(), true);
  assert.equal(
    (coalesced.contentOffset as { y: number }).y,
    (separate.contentOffset as { y: number }).y,
  );
});

test('external native movement replaces an unacknowledged correction as the next baseline', () => {
  const f = restoringDraft();
  f.extra.value = 264;
  f.tick();
  f.props();
  f.scroll.value = 120; // Search or user scrolling is newer than the correction.
  f.tick();
  f.extra.value = 279 + 1 / 3;
  f.tick();
  const target = (f.props().contentOffset as { y: number }).y;
  assert.ok(Math.abs(target - (135 + 1 / 3)) < 0.001);
});

test('a newer keyboard command retires an unacknowledged padding baseline', () => {
  const f = restoringDraft();
  f.extra.value = 264;
  f.tick();
  f.props();
  f.keyboard(0);
  f.props();
  f.extra.value = 279 + 1 / 3;
  f.tick();
  assert.ok(Math.abs((f.props().contentOffset as { y: number }).y - (291 + 1 / 3)) < 0.001);
});

test('interactive dismissal retires the pending baseline even without a new offset command', () => {
  const f = nativeScrollIntentFixture({
    initialExtra: 91 + 1 / 3,
    initialOffset: 900,
    viewportHeight: 874,
    contentHeight: 2000,
    offset: 26,
  });
  f.props();
  f.keyboard(328);
  f.scroll.value = (f.props().contentOffset as { y: number }).y;
  f.extra.value = 264;
  f.tick();
  f.props();
  f.interactive();
  f.keyboard(0); // The SDK intentionally leaves native scrolling in control.
  assert.equal(f.props().contentOffset, undefined);
  f.extra.value = 279 + 1 / 3;
  f.tick();
  assert.ok(Math.abs((f.props().contentOffset as { y: number }).y - (1217 + 1 / 3)) < 0.001);
});

test('frozen changes cannot retain an old pending correction after unfreeze', () => {
  const f = restoringDraft();
  f.extra.value = 264;
  f.tick();
  f.props();
  f.freeze.value = true;
  f.extra.value = 250;
  f.tick();
  f.freeze.value = false;
  f.extra.value = 260;
  f.tick();
  assert.ok(Math.abs((f.props().contentOffset as { y: number }).y - (415 + 1 / 3)) < 0.001);
});

test('a changed minimum padding floor retires the pending baseline', () => {
  const f = restoringDraft();
  f.extra.value = 264;
  f.tick();
  f.props();
  f.blankSpace.value = 100;
  f.tick();
  f.extra.value = 279 + 1 / 3;
  f.tick();
  assert.ok(Math.abs((f.props().contentOffset as { y: number }).y - (420 + 2 / 3)) < 0.001);
});

test('observed native movement away and back retires the old correction', () => {
  const f = restoringDraft();
  f.extra.value = 264;
  f.tick();
  f.props();
  const observed = f.scroll.value;
  f.scroll.value = 100;
  f.tick();
  f.scroll.value = observed;
  f.tick();
  f.extra.value = 279 + 1 / 3;
  f.tick();
  assert.ok(Math.abs((f.props().contentOffset as { y: number }).y - (420 + 2 / 3)) < 0.001);
});

for (const changed of ['layout', 'size'] as const) {
  test(`a changed ${changed} rejects the old pending baseline and retains native clamping`, () => {
    const f = restoringDraft();
    f.extra.value = 264;
    f.tick();
    f.props();
    f[changed].value = { width: 390, height: changed === 'layout' ? 900 : 800 };
    f.extra.value = 279 + 1 / 3;
    f.tick();
    const max = Math.max(f.size.value.height - f.layout.value.height + 581 + 1 / 3, 0);
    assert.ok(
      Math.abs((f.props().contentOffset as { y: number }).y - Math.min(420 + 2 / 3, max)) < 0.001,
    );
  });
}

test('coalesced shrinking and growing padding stays inside the native scroll range', () => {
  const f = restoringDraft();
  f.extra.value = 264;
  f.tick();
  f.extra.value = 0;
  f.tick();
  f.extra.value = 10;
  f.tick();
  const props = f.props();
  assert.equal((props.contentOffset as { y: number }).y, 324);
  assert.equal((props.contentInset as { bottom: number }).bottom, 312);
});
