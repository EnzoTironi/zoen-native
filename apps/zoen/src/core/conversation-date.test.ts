import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { test } from 'node:test';
import { ConversationDateJump, localMessageDay } from './conversation-date.ts';
import { deferred } from './store.fixtures.ts';

const scope = {
  expected_identity_id: 'owner',
  expected_device_id: 'device',
  space_id: 'chat',
  thread_root_id: null,
};
function fixture() {
  let current = true;
  const reads: unknown[] = [],
    reveals: string[] = [],
    selected: string[] = [];
  const response = deferred<string | null>(),
    revealed = deferred<unknown>();
  const owner = new ConversationDateJump(
    scope,
    {
      current: () => current,
      read: async (authority, day) => {
        reads.push({ scope: authority, ...day });
        return response.promise;
      },
      reveal: async (id) => {
        reveals.push(id);
        return revealed.promise;
      },
      select: (id) => {
        selected.push(id);
      },
    },
    new Date(2026, 9, 31, 15).getTime(),
  );
  owner.focus();
  return {
    owner,
    reads,
    reveals,
    selected,
    response,
    revealed,
    retire() {
      current = false;
    },
  };
}

for (const [zone, year, month, day, hours] of [
  ['America/New_York', 2026, 2, 8, 23],
  ['America/New_York', 2026, 10, 1, 25],
  ['America/Sao_Paulo', 2026, 9, 31, 24],
  ['Australia/Lord_Howe', 2026, 9, 4, 23.5],
] as const)
  test(`${zone}: calendar day ${year}-${month + 1}-${day} uses ${hours} hours`, () => {
    const file = new URL('./conversation-date.ts', import.meta.url).href;
    const value = execFileSync(
      process.execPath,
      [
        '--input-type=module',
        '-e',
        `import {localMessageDay} from ${JSON.stringify(file)}; console.log(JSON.stringify(localMessageDay(new Date(${year},${month},${day},15))));`,
      ],
      { env: { ...process.env, TZ: zone }, encoding: 'utf8' },
    );
    const result = JSON.parse(value);
    assert.equal(Number(result.end_ms) - Number(result.start_ms), hours * 3_600_000);
  });

test('calendar rejects invalid and overflowing dates before any Core read', () => {
  for (const date of [new Date(NaN), new Date(-86_400_000), new Date(8_640_000_000_000_000)])
    assert.throws(() => localMessageDay(date));
});

test('one explicit jump reads only scope/day, loads the exact canonical message and selects after reveal', async () => {
  const f = fixture();
  assert.equal(f.owner.open(), true);
  const reviewed = f.owner.getSnapshot();
  const pending = f.owner.jump(reviewed);
  await f.owner.jump(reviewed);
  assert.deepEqual(f.reads, [{ scope, ...localMessageDay(new Date(reviewed.date)) }]);
  assert.deepEqual(f.selected, []);
  f.response.resolve('canonical-id');
  await Promise.resolve();
  await Promise.resolve();
  assert.deepEqual(f.reveals, ['canonical-id']);
  assert.deepEqual(f.selected, []);
  f.revealed.resolve(undefined);
  await pending;
  assert.deepEqual(f.selected, ['canonical-id']);
  assert.equal(f.owner.getSnapshot().presented, false);
});

for (const failure of [null, new Error('private storage failure')])
  test(`calendar ${failure ? 'failure' : 'empty day'} stays open and can be retried`, async () => {
    const f = fixture();
    f.owner.open();
    const pending = f.owner.jump();
    if (failure) f.response.reject(failure);
    else f.response.resolve(null);
    await pending;
    assert.equal(f.owner.getSnapshot().pending, false);
    assert.equal(f.owner.getSnapshot().presented, true);
    assert.equal(f.owner.getSnapshot().error, failure ? 'failed' : 'missing');
    assert.deepEqual(f.reveals, []);
    const next = f.owner.getSnapshot().date + 86_400_000;
    f.owner.setDate(new Date(next));
    assert.equal(f.owner.getSnapshot().error, null);
    await f.owner.jump();
    assert.equal(f.reads.length, 2);
  });

for (const point of ['read', 'reveal'] as const)
  for (const retirement of ['close', 'owner', 'blur'] as const)
    test(`${retirement} during ${point} fences delayed navigation`, async () => {
      const f = fixture();
      f.owner.open();
      const pending = f.owner.jump();
      if (point === 'reveal') {
        f.response.resolve('old');
        await Promise.resolve();
        await Promise.resolve();
      }
      if (retirement === 'close') f.owner.close();
      else if (retirement === 'blur') f.owner.blur();
      else f.retire();
      f.response.resolve('old');
      f.revealed.resolve(undefined);
      await pending;
      assert.deepEqual(f.selected, []);
      if (point === 'read') assert.deepEqual(f.reveals, []);
    });

test('late dismiss/date/Go from an old sheet cannot modify a new presentation', async () => {
  const f = fixture();
  f.owner.open();
  const old = f.owner.getSnapshot();
  f.owner.close(old.presentation);
  f.owner.open();
  const fresh = f.owner.getSnapshot();
  f.owner.close(old.presentation);
  f.owner.setDate(new Date(old.date + 86_400_000), old.presentation);
  await f.owner.jump(old);
  assert.equal(f.owner.getSnapshot(), fresh);
  assert.deepEqual(f.reads, []);
});

test('invalid native date keeps the last valid day and never reads or reveals', () => {
  const f = fixture();
  f.owner.open();
  const date = f.owner.getSnapshot().date;
  f.owner.setDate(new Date(NaN));
  assert.equal(f.owner.getSnapshot().date, date);
  assert.equal(f.owner.getSnapshot().error, 'failed');
  assert.deepEqual(f.reads, []);
});
