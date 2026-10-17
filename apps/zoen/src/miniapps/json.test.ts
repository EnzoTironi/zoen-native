import test from 'node:test';
import assert from 'node:assert/strict';
import { parseMiniappJson, parseMiniappRequest, stringifyMiniappRequest } from './json.ts';
import { photoArguments } from './photo-tokens.ts';

test('miniapp JSON preserves i64 and unsafe fractions with no rounded intermediate values', () => {
  assert.deepEqual(
    parseMiniappJson(
      '{"cents":9223372036854775807,"negative":-9223372036854775808,"safe":9007199254740991,"fraction":0.5,"tiny":1e-999,"precise":0.123456789012345678901}',
    ),
    {
      cents: '9223372036854775807',
      negative: '-9223372036854775808',
      safe: 9007199254740991,
      fraction: 0.5,
      tiny: '1e-999',
      precise: '0.123456789012345678901',
    },
  );
  assert.throws(() => parseMiniappJson('{"price":1,"price":2}'));
});

test('request numeric tokens survive parsing, photo traversal and serialization exactly', () => {
  const encoded =
    '{"cents":9223372036854775807,"text":"9223372036854775807","fraction":0.123456789012345678901,"safe":4}';
  const request = parseMiniappRequest(encoded);
  const traversed = photoArguments(request, 'it_one', () => null).value;
  assert.equal(stringifyMiniappRequest(traversed), encoded);
});
