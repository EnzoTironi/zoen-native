import assert from 'node:assert/strict';
import { test } from 'node:test';
import { money, parseMoneyInput } from './presentation.ts';

test('money input preserves exact cents through the signed 64-bit limit', () => {
  assert.equal(parseMoneyInput('1.234,56'), '123456');
  assert.equal(parseMoneyInput('0,1'), '10');
  assert.equal(parseMoneyInput('92.233.720.368.547.758,07'), '9223372036854775807');
  assert.throws(() => parseMoneyInput('92233720368547758,08'));
  for (const invalid of ['1.2', '1,234', '-1', '1e3', 'NaN', '', '1.00,00'])
    assert.throws(() => parseMoneyInput(invalid));
  assert.equal(money('9223372036854775807'), 'R$ 92.233.720.368.547.758,07');
  assert.equal(money('-1'), '−R$ 0,01');
});
