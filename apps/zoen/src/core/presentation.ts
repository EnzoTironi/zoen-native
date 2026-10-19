import type { TimelineEntry } from './types';
import { locale, t } from '../i18n/index.ts';
import { AmountError, formatMoney, parseMoneyInput as parseAmount } from '../i18n/money.ts';

export function messageText(entry: TimelineEntry): string {
  if (entry.deleted) return t('presentation.deletedMessage');
  if ('Message' in entry.kind) return entry.kind.Message.text;
  if ('System' in entry.kind) return entry.kind.System.text;
  if ('ItemEdited' in entry.kind) return entry.kind.ItemEdited.note;
  if ('Request' in entry.kind) return entry.kind.Request.title;
  return t('presentation.chatAppearanceUpdated');
}

export function money(cents: string): string {
  // Plans and the current grant allowance are denominated in BRL cents by the core.
  return formatMoney(cents, { locale, currency: 'BRL', minorUnitDigits: 2 });
}

/** New plan costs and grant allowances remain nonnegative signed-i64 BRL cents. */
export function parseMoneyInput(input: string): string {
  return parseCents(input, false);
}

/** A historical negative plan value can round-trip; Rust refuses any new negative cost. */
export function parsePlanMoneyInput(input: string): string {
  return parseCents(input, true);
}

function parseCents(input: string, allowNegative: boolean): string {
  try {
    return parseAmount(input, {
      locale,
      minorUnitDigits: 2,
      allowNegative,
      min: allowNegative ? -9223372036854775808n : 0n,
      max: 9223372036854775807n,
    });
  } catch (reason) {
    if (!(reason instanceof AmountError)) throw reason;
    throw new Error(
      t(
        reason.code === 'out_of_range'
          ? 'presentation.amountOutOfRange'
          : 'presentation.enterBRLAmount',
      ),
    );
  }
}
