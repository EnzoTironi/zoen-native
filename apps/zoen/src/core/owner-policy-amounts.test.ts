import assert from 'node:assert/strict';
import test from 'node:test';
import { policyAmountInput, policyDraft } from '../agents/owner-policy-amounts.ts';

test('localized inputs preserve every micro-unit through the existing exact money parser', () => {
  for (const [locale, amount] of [
    ['en', '9,223,372,036,854.775807'],
    ['pt-BR', '9.223.372.036.854,775807'],
  ] as const) {
    const result = policyDraft(
      amount,
      locale === 'en' ? '0.000011' : '0,000011',
      true,
      6,
      '10',
      locale,
    );
    assert.deepEqual(result, {
      draft: { limit_units: '9223372036854775807', max_attempt_units: '11', enabled: true },
      error: null,
    });
    const text = policyAmountInput(result.draft!.limit_units, 6, locale);
    assert.equal(
      policyDraft(text, text, true, 6, '10', locale).draft?.limit_units,
      '9223372036854775807',
    );
  }
  assert.equal(policyAmountInput('1000000', 6, 'en'), '1.00');
  assert.equal(policyAmountInput('11', 6, 'pt-BR'), '0,000011');
});

test('partial, exponent, rounded, negative and overflowing drafts cannot reach review', () => {
  for (const amount of ['', '1.', '-1', '1e3', '0.0000001', '9223372036854.775808']) {
    assert.deepEqual(policyDraft(amount, '0', true, 6, '0', 'en'), {
      draft: null,
      error: 'invalid',
    });
  }
  assert.deepEqual(policyDraft('1', '2', true, 6, '0', 'en'), { draft: null, error: 'attempt' });
  assert.deepEqual(policyDraft('1', '0.000009', true, 6, '10', 'en'), {
    draft: null,
    error: 'reservation',
  });
  assert.deepEqual(policyDraft('0', '0', false, 6, '10', 'en'), {
    draft: { limit_units: '0', max_attempt_units: '0', enabled: false },
    error: null,
  });
  assert.deepEqual(policyDraft('1', '1', true, 6, 'not-an-amount', 'en'), {
    draft: null,
    error: 'invalid',
  });
});
