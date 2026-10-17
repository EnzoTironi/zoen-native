import test from 'node:test';
import assert from 'node:assert/strict';
import {
  PhotoVault,
  MAX_PHOTO_DATA_URL,
  busyBlocks,
  eventDraft,
  photoOptions,
  pickedContact,
  place,
  timeWindow,
} from './capability-data.ts';

const preview = {
  dataUrl: 'data:image/jpeg;base64,YQ==',
  width: 1024,
  height: 768,
  locationStripped: true as const,
};
const nonce = (value: number) => value.toString(16).padStart(32, '0');

test('private photos bind to one item, expire, and teardown invalidates an in-flight picker', () => {
  let now = 0;
  const vault = new PhotoVault(() => now);
  const lease = vault.begin('item-a');
  const photo = vault.put(lease, nonce(1), preview);
  assert.equal(vault.resolve(photo.token, 'item-b'), null);
  assert.equal(vault.resolve(photo.token, 'item-a'), preview.dataUrl);
  now = 15 * 60_000;
  assert.equal(vault.resolve(photo.token, 'item-a'), null);
  const second = vault.put(lease, nonce(2), preview);
  vault.release('item-a');
  assert.equal(lease.signal.aborted, true);
  assert.equal(vault.resolve(second.token, 'item-a'), null);
  assert.throws(() => vault.put(lease, nonce(3), preview), /fechado/);
  const reopened = vault.begin('item-a');
  vault.put(reopened, nonce(4), preview);
  assert.throws(() => lease.check(), /fechado/);
});

test('a miniapp cannot retain unbounded full images or evade preview size constraints', () => {
  const vault = new PhotoVault();
  const lease = vault.begin('item');
  const first = vault.put(lease, nonce(0), preview);
  for (let index = 1; index <= 12; index++) vault.put(lease, nonce(index), preview);
  assert.equal(vault.resolve(first.token, 'item'), null);
  assert.throws(() =>
    vault.put(lease, nonce(14), {
      ...preview,
      dataUrl: 'data:image/jpeg;base64,' + 'a'.repeat(MAX_PHOTO_DATA_URL),
    }),
  );
  assert.throws(() => vault.put(lease, nonce(15), { ...preview, width: 10_000 }));
  assert.throws(() =>
    vault.put(lease, nonce(16), { ...preview, dataUrl: 'data:image/svg+xml,<svg/>' }),
  );
  assert.deepEqual(photoOptions({ max: 100, maxSide: 9999 }), { max: 6, maxSide: 1600 });
  assert.throws(() => photoOptions({ max: NaN }));
});

test('freebusy returns only clipped, merged timed intervals within a 31-day window', () => {
  const range = timeWindow({ startMs: 100, endMs: 40 * 86_400_000 });
  assert.equal(range.endMs, 100 + 31 * 86_400_000);
  const blocks = busyBlocks(
    [
      { startMs: 150, endMs: 200 },
      { startMs: 50, endMs: 160 },
      { startMs: 190, endMs: 220 },
      { startMs: 110, endMs: 230, free: true },
      { startMs: 110, endMs: 250, allDay: true },
      { startMs: NaN, endMs: 1000 },
      { startMs: range.endMs - 10, endMs: range.endMs + 20 },
    ],
    range,
  );
  assert.deepEqual(blocks, [
    { startMs: 100, endMs: 220 },
    { startMs: range.endMs - 10, endMs: range.endMs },
  ]);
  assert.throws(() => timeWindow({ startMs: 10, endMs: 10 }));
  assert.throws(() => timeWindow({ startMs: 10, endMs: Number.MAX_SAFE_INTEGER }));
});

test('system calendar drafts validate dates and bound text before presenting any write UI', () => {
  const draft = eventDraft({
    title: '  Caminhada  ',
    startMs: 1000,
    endMs: 2000,
    location: 'a'.repeat(200),
    notes: 'b'.repeat(1200),
  });
  assert.equal(draft.title, 'Caminhada');
  assert.equal(draft.location?.length, 160);
  assert.equal(draft.notes?.length, 1000);
  assert.equal(draft.startDate.getTime(), 1000);
  assert.throws(() => eventDraft({ title: 'a', startMs: 1000, endMs: 1000 + 32 * 86_400_000 }));
  assert.throws(() => eventDraft({ title: '\0', startMs: 1000, endMs: 2000 }));
  assert.throws(() => eventDraft({ title: '', startMs: 1000, endMs: 2000 }));
});

test('approximate location removes exact coordinates and selected contacts expose name only', () => {
  const exact = place(-23.55321, -46.63432, 5, false);
  const coarse = place(-23.55321, -46.63432, 5, true);
  assert.equal(exact.approximate, false);
  assert.notEqual(coarse.lat, exact.lat);
  assert.notEqual(coarse.lon, exact.lon);
  assert.equal(coarse.accuracyM, 3000);
  assert.equal(place(1, 2, 5000, true).accuracyM, 5000);
  assert.throws(() => place(NaN, 2, 5, false));
  assert.deepEqual(pickedContact('  Ana\0 Clara  '), [{ name: 'Ana  Clara', initials: 'AC' }]);
  assert.deepEqual(pickedContact(''), []);
});
