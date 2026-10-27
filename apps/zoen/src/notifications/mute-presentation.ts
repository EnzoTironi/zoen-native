import { t, formatLocale } from '../i18n/index.ts';

export const PERMANENT_MUTE = '9223372036854775807';

export function muteDate(value: string | null): Date | null {
  if (!value || !/^[1-9][0-9]*$/.test(value) || value === PERMANENT_MUTE) return null;
  const date = new Date(Number(value));
  return Number.isFinite(date.getTime()) ? date : null;
}

/** Display only: expiry and notification admission are always projected by Core. */
export function muteSummary(until: string | null): string {
  if (until === PERMANENT_MUTE) return t('notifications.muteForever');
  const date = muteDate(until);
  return date
    ? t('notifications.mutedUntil', {
        date: date.toLocaleString(formatLocale, { dateStyle: 'short', timeStyle: 'short' }),
      })
    : t('notifications.notificationsOn');
}
