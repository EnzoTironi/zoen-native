import test from 'node:test';
import assert from 'node:assert/strict';
import { AmountError, formatMoney, formatMoneyInput, parseMoneyInput } from '../i18n/money.ts';
const max = 9223372036854775807n;
const en = { locale: 'en' as const, minorUnitDigits: 2, currency: 'USD' };
const pt = { locale: 'pt-BR' as const, minorUnitDigits: 2, currency: 'BRL' };
const normalized = (value: string) => value.replaceAll('\u00a0', ' ');
test('formatting retains every exact i64-cent digit without Number coercion', () => {
  assert.equal(formatMoney(max.toString(), en), '$92,233,720,368,547,758.07');
  assert.equal(normalized(formatMoney(max.toString(), pt)), 'R$ 92.233.720.368.547.758,07');
  assert.equal(formatMoney('-1', en), '-$0.01');
  assert.equal(normalized(formatMoney('-1', pt)), '-R$ 0,01');
  assert.equal(formatMoney('0', en), '$0.00');
});
test('currency and scale remain explicit and independent of language', () => {
  assert.equal(normalized(formatMoney('12345', { ...pt, currency: 'USD' })), 'US$ 123,45');
  assert.equal(formatMoney('12345', { ...en, currency: 'BRL' }), 'R$123.45');
  assert.equal(formatMoney('12345', { ...en, currency: 'JPY', minorUnitDigits: 0 }), '¥12,345');
  assert.equal(
    formatMoney('12345', { ...en, currency: 'KWD', minorUnitDigits: 3 }),
    'KWD\u00a012.345',
  );
  assert.throws(
    () => formatMoney('1', { ...en, currency: '' }),
    (e: unknown) => e instanceof AmountError && e.code === 'invalid_currency',
  );
});
test('localized input accepts valid grouping, exact fractions and explicit wire limits', () => {
  assert.equal(parseMoneyInput('92,233,720,368,547,758.07', { ...en, max }), max.toString());
  assert.equal(parseMoneyInput('92.233.720.368.547.758,07', { ...pt, max }), max.toString());
  assert.equal(parseMoneyInput(' 12,5 ', pt), '1250');
  assert.equal(parseMoneyInput('12.5', en), '1250');
  assert.equal(parseMoneyInput('−0,01', { ...pt, allowNegative: true }), '-1');
  assert.throws(
    () => parseMoneyInput('92233720368547758.08', { ...en, max }),
    (e: unknown) => e instanceof AmountError && e.code === 'out_of_range',
  );
});
test('wrong separators, malformed grouping, excess precision and coercion tricks are refused', () => {
  for (const bad of [
    '1,23',
    '1.234',
    '1e3',
    'Infinity',
    'NaN',
    '$1.00',
    '+1',
    '-1',
    '1,000,',
    '1.',
    '',
    '1_000',
  ])
    assert.throws(() => parseMoneyInput(bad, en), bad);
  for (const bad of ['1.23', '1,234', 'R$ 1,00', '1.000.', '1,', '1 000,00'])
    assert.throws(() => parseMoneyInput(bad, pt), bad);
});
test('editable text roundtrips extremes in both languages and supported scales', () => {
  for (const locale of ['en', 'pt-BR'] as const)
    for (const minorUnitDigits of [0, 2, 3, 8])
      for (const value of ['0', '1', '-1', max.toString(), (-max - 1n).toString()]) {
        const options = { locale, minorUnitDigits, allowNegative: true };
        assert.equal(parseMoneyInput(formatMoneyInput(value, options), options), value);
      }
});
