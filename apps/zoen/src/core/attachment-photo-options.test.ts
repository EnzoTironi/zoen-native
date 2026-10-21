import assert from 'node:assert/strict';
import { test } from 'node:test';
import { photoSendOptions } from '../media/attachment-photo-options.ts';

test('fresh Photos default groups two through ten items, while a single photo keeps its legacy envelope', () => {
  assert.equal(photoSendOptions(1, 'default'), undefined);
  for (let count = 2; count <= 10; count++) {
    assert.deepEqual(photoSendOptions(count, 'default'), {
      presentation: 'media',
      grouping: 'grouped',
    });
    assert.deepEqual(photoSendOptions(count, 'send-without-grouping'), {
      presentation: 'media',
      grouping: 'separate',
    });
    assert.deepEqual(photoSendOptions(count, 'send-as-file'), {
      presentation: 'file',
      grouping: 'separate',
    });
  }
  for (const count of [0, -1, 11, 1.5, NaN, Infinity])
    assert.throws(() => photoSendOptions(count, 'default'));
  assert.throws(() => photoSendOptions(1, 'send-without-grouping'));
});

test('append inherits the existing admitted batch and cannot convert its presentation or grouping', () => {
  for (const options of [
    { presentation: 'media', grouping: 'grouped' },
    { presentation: 'media', grouping: 'separate' },
    { presentation: 'file', grouping: 'separate' },
  ] as const) {
    assert.deepEqual(photoSendOptions(1, 'default', options), options);
    assert.notStrictEqual(photoSendOptions(2, 'default', options), options);
  }
  assert.throws(() =>
    photoSendOptions(2, 'send-as-file', { presentation: 'media', grouping: 'grouped' }),
  );
  assert.throws(() =>
    photoSendOptions(2, 'send-without-grouping', { presentation: 'media', grouping: 'grouped' }),
  );
  assert.throws(() =>
    photoSendOptions(2, 'send-without-grouping', { presentation: 'file', grouping: 'separate' }),
  );
});
