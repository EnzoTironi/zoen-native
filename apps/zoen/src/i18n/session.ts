import { getLocales } from 'expo-localization';
import { initializeLocalization } from './index.ts';
import { resolveLocale } from '../../../../packages/zoen-ui/web/locale.ts';

// The custom Expo entry imports this before Router loads any route module.
// iOS language changes restart the app; Web chooses preferences at page load.
// Android foreground changes later need a coordinated reopen of all core owners.
export const appLocale = resolveLocale(getLocales().map((locale) => locale.languageTag));
initializeLocalization(appLocale);
