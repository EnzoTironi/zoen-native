import { formatLocale } from '../../../../packages/zoen-ui/web/locale.ts';
import type { AppLocale } from '../../../../packages/zoen-ui/web/locale.ts';

export class AmountError extends Error {
  readonly code: 'invalid_amount' | 'out_of_range' | 'invalid_currency' | 'invalid_scale';
  constructor(code: AmountError['code']) {
    super(code);
    this.name = 'AmountError';
    this.code = code;
  }
}
type Scale = { locale: AppLocale; minorUnitDigits: number };
export type MoneyOptions = Scale & { currency: string };
function scale(value: number): bigint {
  if (!Number.isInteger(value) || value < 0 || value > 8) throw new AmountError('invalid_scale');
  return 10n ** BigInt(value);
}
function amount(value: string): bigint {
  if (!/^-?\d{1,256}$/.test(value)) throw new AmountError('invalid_amount');
  return BigInt(value);
}
function separators(locale: AppLocale) {
  const parts = new Intl.NumberFormat(formatLocale(locale), {
    minimumFractionDigits: 1,
  }).formatToParts(1234);
  return {
    decimal: parts.find((part) => part.type === 'decimal')!.value,
    group: parts.find((part) => part.type === 'group')!.value,
  };
}

/** No exchange rates, implicit currency or Number(cents). Caller supplies currency and scale.
 * Existing *_cents fields pass minorUnitDigits: 2. Other domain fields must supply their own scale.
 * Intl positions the sign/currency; BigInt produces every digit, including the sub-unit remainder.
 */
export function formatMoney(value: string, options: MoneyOptions): string {
  const factor = scale(options.minorUnitDigits);
  if (!/^[A-Z]{3}$/.test(options.currency)) throw new AmountError('invalid_currency');
  const minor = amount(value),
    absolute = minor < 0n ? -minor : minor;
  const integer = new Intl.NumberFormat(formatLocale(options.locale), {
    maximumFractionDigits: 0,
  }).format(absolute / factor);
  const fraction = (absolute % factor).toString().padStart(options.minorUnitDigits, '0');
  const template = new Intl.NumberFormat(formatLocale(options.locale), {
    style: 'currency',
    currency: options.currency,
    minimumFractionDigits: options.minorUnitDigits,
    maximumFractionDigits: options.minorUnitDigits,
  }).formatToParts(minor < 0n ? -1n : 1n);
  let inserted = false;
  return template
    .map((part) => {
      if (part.type === 'integer' || part.type === 'group') {
        if (inserted) return '';
        inserted = true;
        return integer;
      }
      return part.type === 'fraction' ? fraction : part.value;
    })
    .join('');
}
/** Editable decimal text has no currency/grouping; the visible field label names its currency. */
export function formatMoneyInput(value: string, options: Scale): string {
  const factor = scale(options.minorUnitDigits);
  const minor = amount(value),
    absolute = minor < 0n ? -minor : minor;
  const integer = `${minor < 0n ? '-' : ''}${absolute / factor}`;
  return options.minorUnitDigits
    ? `${integer}${separators(options.locale).decimal}${String(absolute % factor).padStart(options.minorUnitDigits, '0')}`
    : integer;
}

/** Input grammar follows the chosen UI locale; Rust still owns all financial policy.
 * A cents wire caller can pass max: 9223372036854775807n. No rounding/truncation is accepted.
 */
export function parseMoneyInput(
  input: string,
  options: Scale & { allowNegative?: boolean; min?: bigint; max?: bigint },
): string {
  const factor = scale(options.minorUnitDigits);
  const text = input.trim();
  if (!text || text.length > 256) throw new AmountError('invalid_amount');
  const { decimal, group } = separators(options.locale);
  const negative = text.startsWith('-') || text.startsWith('−');
  if (negative && !options.allowNegative) throw new AmountError('invalid_amount');
  const unsigned = negative ? text.slice(1) : text;
  const parts = unsigned.split(decimal);
  if (parts.length > 2 || !parts[0] || parts[1] === '') throw new AmountError('invalid_amount');
  const groups = parts[0].split(group);
  if (
    groups.some(
      (digits, index) =>
        !/^\d+$/.test(digits) ||
        (groups.length > 1 && (index === 0 ? digits.length > 3 : digits.length !== 3)),
    )
  )
    throw new AmountError('invalid_amount');
  const fraction = parts[1] ?? '';
  if ((fraction && !/^\d+$/.test(fraction)) || fraction.length > options.minorUnitDigits)
    throw new AmountError('invalid_amount');
  const absolute =
    BigInt(groups.join('')) * factor + BigInt(fraction.padEnd(options.minorUnitDigits, '0') || '0');
  const result = negative ? -absolute : absolute;
  if (
    (options.min !== undefined && result < options.min) ||
    (options.max !== undefined && result > options.max)
  )
    throw new AmountError('out_of_range');
  return result.toString();
}
