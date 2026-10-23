import { formatMoney } from '../i18n/money.ts';

/** Reuse native-safe money rendering; never pass a financial value to Number. */
export function policyAmount(
  units: string,
  currency: string,
  scale: number,
  locale: string,
): string | null {
  if (
    !/^(0|[1-9][0-9]{0,18})$/.test(units) ||
    BigInt(units) > 9_223_372_036_854_775_807n ||
    !/^[A-Z]{3}$/.test(currency) ||
    !Number.isInteger(scale) ||
    scale < 0 ||
    scale > 8 ||
    (locale !== 'en-US' && locale !== 'pt-BR')
  )
    return null;
  return formatMoney(units, {
    locale: locale === 'en-US' ? 'en' : 'pt-BR',
    currency,
    minorUnitDigits: scale,
  }).replace(/([.,]\d{2,}?)0+(?=\D*$)/, '$1');
}

/** Narrow presentation input; the Core review and confirmation digest own authority. */
export type OwnerPolicyReviewFields = Readonly<{
  currency: string;
  scale: number;
  year: number;
  month: number;
  limit_units: string;
  max_attempt_units: string;
  expires_at_ms: string;
  enabled: boolean;
  allowed_profiles: readonly string[];
}>;

export type PolicyProfileLabel = Readonly<{ digest: string; label: string }>;

export function policyPresentation(
  review: OwnerPolicyReviewFields,
  profiles: readonly PolicyProfileLabel[],
  locale: string,
) {
  const limit = policyAmount(review.limit_units, review.currency, review.scale, locale);
  const attempt = policyAmount(review.max_attempt_units, review.currency, review.scale, locale);
  const expires = Number(review.expires_at_ms);
  if (
    !limit ||
    !attempt ||
    BigInt(review.max_attempt_units) > BigInt(review.limit_units) ||
    !Number.isInteger(review.year) ||
    review.year < 2000 ||
    review.year > 2200 ||
    !Number.isInteger(review.month) ||
    review.month < 1 ||
    review.month > 12 ||
    !/^[1-9][0-9]{0,15}$/.test(review.expires_at_ms) ||
    !Number.isSafeInteger(expires) ||
    expires > 8_640_000_000_000_000 ||
    review.allowed_profiles.length === 0 ||
    review.allowed_profiles.length > 16
  )
    return null;
  const labels: string[] = [];
  const seen = new Set<string>();
  for (const digest of review.allowed_profiles) {
    const matches = profiles.filter((profile) => profile.digest === digest);
    if (
      !/^[a-f0-9]{64}$/.test(digest) ||
      seen.has(digest) ||
      matches.length !== 1 ||
      !matches[0]!.label.trim()
    )
      return null;
    seen.add(digest);
    labels.push(matches[0]!.label);
  }
  return {
    limit,
    attempt,
    profiles: labels,
    period: new Intl.DateTimeFormat(locale, {
      month: 'long',
      year: 'numeric',
      timeZone: 'UTC',
    }).format(Date.UTC(review.year, review.month - 1, 1)),
    expires: new Intl.DateTimeFormat(locale, {
      dateStyle: 'medium',
      timeStyle: 'short',
      timeZone: 'UTC',
    }).format(expires),
  };
}
