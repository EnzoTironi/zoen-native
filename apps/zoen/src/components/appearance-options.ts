import { t } from '../i18n/index.ts';
import type { SkyPhase, ThemePreference } from '../theme/sky.ts';
export function skyLabels(): Record<ThemePreference, string> {
  return {
    auto: t('settings.skyAuto'),
    day: t('settings.skyDay'),
    night: t('settings.skyNight'),
  };
}
export function currentSkyLabel(preference: ThemePreference, phase: SkyPhase): string {
  const labels = skyLabels();
  return preference === 'auto' ? `${labels.auto} · ${labels[phase]}` : labels[preference];
}
