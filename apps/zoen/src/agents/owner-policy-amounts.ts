import { AmountError, formatMoneyInput, parseMoneyInput } from '../i18n/money.ts';

export type PolicyAmountDraft = {
  limit_units: string;
  max_attempt_units: string;
  enabled: boolean;
};
type Locale = Parameters<typeof parseMoneyInput>[1]['locale'];
const max = 9_223_372_036_854_775_807n;

export function policyAmountInput(units: string, scale: number, locale: Locale): string {
  if (!units) return '';
  return formatMoneyInput(units, { locale, minorUnitDigits: scale }).replace(
    /([.,]\d{2,}?)0+$/,
    '$1',
  );
}

/** UI input only. Core rechecks the authenticated quote and signs its own review. */
export function policyDraft(
  limit: string,
  attempt: string,
  enabled: boolean,
  scale: number,
  reservation: string,
  locale: Locale,
): { draft: PolicyAmountDraft | null; error: 'invalid' | 'attempt' | 'reservation' | null } {
  try {
    const options = { locale, minorUnitDigits: scale, min: 0n, max };
    const limit_units = parseMoneyInput(limit, options);
    const max_attempt_units = parseMoneyInput(attempt, options);
    if (!/^(0|[1-9][0-9]{0,18})$/.test(reservation) || BigInt(reservation) > max)
      return { draft: null, error: 'invalid' };
    if (BigInt(max_attempt_units) > BigInt(limit_units)) return { draft: null, error: 'attempt' };
    if (
      enabled &&
      (BigInt(limit_units) < BigInt(reservation) || BigInt(max_attempt_units) < BigInt(reservation))
    )
      return { draft: null, error: 'reservation' };
    return { draft: { limit_units, max_attempt_units, enabled }, error: null };
  } catch (reason) {
    if (!(reason instanceof AmountError)) throw reason;
    return { draft: null, error: 'invalid' };
  }
}
