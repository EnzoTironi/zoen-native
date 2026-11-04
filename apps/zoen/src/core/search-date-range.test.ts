import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { test } from 'node:test';
import { SearchDateRange, searchDateRange } from './search-date-range.ts';

test('custom range admits reviewed dates only and cannot revive a dismissed, blurred or retired panel', () => {
  const values: unknown[] = [];
  let current = true;
  const range = new SearchDateRange({
    current: () => current,
    apply: (value) => values.push(value),
  });
  assert.equal(range.open(), false);
  range.focus();
  assert.equal(range.open(undefined, new Date(2026, 9, 4).getTime()), true);
  const first = range.getSnapshot();
  range.pick(first.token, 'start', new Date(2026, 9, 1));
  range.pick(first.token, 'end', new Date(2026, 9, 5));
  assert.equal(
    range.apply(first),
    false,
    'old rendered Apply does not accept newly changed fields',
  );
  const reviewed = range.getSnapshot();
  assert.equal(range.apply(reviewed), true);
  assert.deepEqual(values, [searchDateRange(new Date(2026, 9, 1), new Date(2026, 9, 5))]);
  assert.equal(range.apply(reviewed), false);
  range.open();
  const next = range.getSnapshot();
  range.close(first.token);
  assert.equal(
    range.getSnapshot(),
    next,
    'a late sheet dismissal cannot close another presentation',
  );
  current = false;
  range.pick(next.token, 'start', new Date(2026, 0, 1));
  assert.equal(range.getSnapshot(), next);
  assert.equal(range.apply(next), false);
  current = true;
  range.blur();
  range.focus();
  assert.equal(range.apply(next), false, 'focus does not revive the closed presentation');
});

test('calendar ranges include both local days, reject invalid/reversed days and retain the latest binding', () => {
  assert.equal(searchDateRange(new Date(NaN), new Date()), null);
  assert.equal(searchDateRange(new Date(2026, 1, 2), new Date(2026, 1, 1)), null);
  assert.equal(searchDateRange(new Date(1960, 1, 1), new Date(1960, 1, 2)), null);
  const range = new SearchDateRange();
  const first: unknown[] = [],
    second: unknown[] = [];
  const release = range.bind({ current: () => true, apply: (value) => first.push(value) });
  const stop = range.bind({ current: () => true, apply: (value) => second.push(value) });
  release();
  range.focus();
  range.open();
  assert.equal(range.apply(range.getSnapshot()), true);
  assert.equal(first.length, 0);
  assert.equal(second.length, 1);
  range.open();
  const old = range.getSnapshot();
  stop();
  assert.equal(range.apply(old), false);
});

for (const [zone, year, month, day, hours] of [
  ['America/New_York', 2026, 2, 8, 23],
  ['America/New_York', 2026, 10, 1, 25],
  ['America/Sao_Paulo', 2026, 9, 1, 24],
] as const) {
  test(`custom range includes the complete local day across ${hours}h in ${zone}`, () => {
    const source = new URL('./search-date-range.ts', import.meta.url).href;
    const result = execFileSync(
      process.execPath,
      [
        '--input-type=module',
        '-e',
        `import {searchDateRange} from ${JSON.stringify(source)}; const r=searchDateRange(new Date(${year},${month},${day}),new Date(${year},${month},${day})); console.log(JSON.stringify(r));`,
      ],
      { env: { ...process.env, TZ: zone }, encoding: 'utf8' },
    );
    const range = JSON.parse(result);
    assert.equal((range.end_ms - range.start_ms) / 3_600_000, hours);
  });
}
