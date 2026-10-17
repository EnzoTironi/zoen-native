import { t } from '../i18n/index.ts';
import type { SkyPhase, ThemePreference } from '../theme/sky.ts';
export function skyLabels(): Record<ThemePreference, string> {
  return {
    auto: t('settings.skyAuto'),
    deepNight: t('settings.skyDeepNight'),
    dawn: t('settings.skyDawn'),
    morning: t('settings.skyMorning'),
    day: t('settings.skyDay'),
    sunset: t('settings.skySunset'),
    night: t('settings.skyNight'),
  };
}
export function currentSkyLabel(preference: ThemePreference, phase: SkyPhase): string {
  const labels = skyLabels();
  return preference === 'auto' ? `${labels.auto} · ${labels[phase]}` : labels[preference];
}
