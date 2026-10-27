import assert from 'node:assert/strict';
import test from 'node:test';
import { createThemeInputProbe } from '../theme/input-probe.ts';

test('disabled theme probe is inert and enabled counters are closed and bounded', () => {
  const lines: string[] = [];
  const disabled = createThemeInputProbe(false, (line) => lines.push(line));
  for (let i = 0; i < 100; i++) disabled('js-callback');
  assert.equal(lines.length, 0);
  const enabled = createThemeInputProbe(true, (line) => lines.push(line));
  enabled('unexpected private value' as never);
  for (const stage of [
    'screen-touch-start',
    'js-callback',
    'preference-request',
    'preference-committed',
  ] as const)
    for (let i = 0; i < 100; i++) enabled(stage);
  assert.equal(lines.length, 32);
  assert.ok(
    lines.every((line) =>
      /^\[ZoenThemeInput\] (screen-touch-start|js-callback|preference-request|preference-committed) count=[1-8]$/.test(
        line,
      ),
    ),
  );
});

test('a failing diagnostic sink never blocks the input callback', () => {
  const probe = createThemeInputProbe(true, () => {
    throw new Error('unavailable synthetic sink');
  });
  assert.doesNotThrow(() => probe('js-callback'));
});
