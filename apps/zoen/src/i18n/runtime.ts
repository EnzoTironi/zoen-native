import { I18n, useMakePlural as makePluralRule } from 'i18n-js';
import { en, pt } from 'make-plural';
import { assertCatalogParity } from './catalog.ts';
import type { Args, At, FeatureCatalog, LeafPaths } from './catalog.ts';
import { resolveLocale } from '../../../../packages/zoen-ui/web/locale.ts';
import type { AppLocale } from '../../../../packages/zoen-ui/web/locale.ts';

type Catalogs = Readonly<Record<string, FeatureCatalog>>;
type English<C extends Catalogs> = { [K in keyof C]: C[K]['en'] };
/** The library handles interpolation, plurals and fallback. The wrapper adds key/parameter types. */
export function createLocalization<const C extends Catalogs>(
  catalogs: C,
  preferences: readonly string[],
) {
  const translations: Record<AppLocale, Record<string, unknown>> = { en: {}, 'pt-BR': {} };
  for (const [feature, catalog] of Object.entries(catalogs)) {
    assertCatalogParity(catalog);
    translations.en[feature] = catalog.en;
    translations['pt-BR'][feature] = catalog['pt-BR'];
  }
  const locale = resolveLocale(preferences);
  const i18n = new I18n(translations, {
    locale,
    defaultLocale: 'en',
    enableFallback: true,
    missingBehavior: 'error',
  });
  for (const [tag, pluralizer] of [
    ['en', en],
    ['pt-BR', pt],
  ] as const) {
    const select = makePluralRule({ pluralizer });
    i18n.pluralization.register(tag, (instance, count) => [
      ...new Set([...select(instance, count), 'other']),
    ]);
  }
  function t<K extends LeafPaths<English<C>>>(key: K, ...args: Args<At<English<C>, K>>): string {
    return i18n.t(key, args[0]);
  }
  // Deliberately fixed per session: no silent JS-only language switch while native
  // engine/workers still speak the previous language. A future switch coordinates those owners.
  return { locale, t };
}
