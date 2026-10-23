import { t } from '../i18n/index.ts';
import type { SkyPhase, ThemePreference } from '../theme/sky.ts';
export function skyLabels(): Record<ThemePreference, string> {
  return {
    auto: t('settings.skyAuto'),
    day: t('settings.skyDay'),
    night: t('settings.skyNight'),
    'telegram-classic': t('settings.themeClassic'),
    'telegram-day': t('settings.themeDay'),
    'telegram-night': t('settings.themeNight'),
    'telegram-tinted': t('settings.themeTinted'),
  };
}
export function currentSkyLabel(preference: ThemePreference, phase: SkyPhase): string {
  const labels = skyLabels();
  return preference === 'auto' ? `${labels.auto} · ${labels[phase]}` : labels[preference];
}
