/** Host-independent resolver. Expo supplies getLocales().map(locale => locale.languageTag). */
export type AppLocale = 'en' | 'pt-BR';
export function resolveLocale(preferences: readonly string[]): AppLocale {
  for (const preference of preferences) {
    const language = preference.trim().replaceAll('_', '-').split('-')[0]?.toLowerCase();
    if (language === 'pt') return 'pt-BR';
    if (language === 'en') return 'en';
  }
  return 'en';
}
export function formatLocale(locale: AppLocale): 'en-US' | 'pt-BR' {
  return locale === 'pt-BR' ? 'pt-BR' : 'en-US';
}
/** Provider dialects are presentation only. Currency is deliberately absent. */
export function providerLocale(locale: AppLocale) {
  return { google: formatLocale(locale), apple: locale === 'pt-BR' ? 'pt_BR' : 'en_US' };
}

/** Cross-boundary sessions require a resolved tag, never a provider/device hint. */
export function sessionLocale(value: unknown): AppLocale {
  if (value === 'en' || value === 'pt-BR') return value;
  throw new Error('Invalid session language.');
}
