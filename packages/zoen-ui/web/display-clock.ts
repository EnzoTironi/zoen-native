let fixed: number | undefined;
let timeZone: string | undefined;
export function displayNow(): number { return fixed ?? Date.now(); }
export function isDisplayClockFixed(): boolean { return fixed !== undefined; }
export function configureDisplayClock(config?: { now: number; timeZone: string }) { fixed = config?.now; timeZone = config?.timeZone; }
/** RodaTime.relative, using the same displayed clock as native fixture content. */
export function displayRelativeTime(ms: number): string {
  const seconds = (displayNow() - ms) / 1000;
  if (seconds < 60) return 'now';
  if (seconds < 3600) return `${Math.trunc(seconds / 60)} min`;
  if (seconds < 86_400) return `${Math.trunc(seconds / 3600)} h`;
  const locale = typeof document === 'undefined' ? 'en-US' : document.documentElement.lang || 'en-US';
  const zone = timeZone ? { timeZone } : {};
  const calendar = new Intl.DateTimeFormat('en-US', { year: 'numeric', month: 'numeric', day: 'numeric', ...zone });
  const day = (value: number) => Object.fromEntries(calendar.formatToParts(value).map(part => [part.type, part.value]));
  const today = day(displayNow()), date = day(ms);
  const calendarDay = (parts: Record<string, string>) => Date.UTC(Number(parts.year), Number(parts.month) - 1, Number(parts.day));
  if (calendarDay(today) - calendarDay(date) === 86_400_000) return locale.startsWith('pt') ? 'Ontem' : 'Yesterday';
  // RodaTime.short currently uses the wall clock for this older-date branch.
  if (Math.trunc((Date.now() - ms) / 86_400_000) < 7) {
    const weekday = new Intl.DateTimeFormat(locale, { weekday: 'short', ...zone }).format(ms).replaceAll('.', '');
    return weekday.charAt(0).toUpperCase() + weekday.slice(1);
  }
  return new Intl.DateTimeFormat(locale, { day: 'numeric', month: 'short', ...zone }).format(ms);
}
/** RodaTime.short: calendar dates use the same locale, zone and displayed clock as times. */
export function displayShort(ms: number): string {
  const locale = typeof document === 'undefined' ? 'pt-BR' : document.documentElement.lang || 'pt-BR';
  const zone = timeZone ? { timeZone } : {};
  const calendar = new Intl.DateTimeFormat('en-US', { year: 'numeric', month: 'numeric', day: 'numeric', ...zone });
  const day = (value: number) => {
    const parts = Object.fromEntries(calendar.formatToParts(value).map(part => [part.type, part.value]));
    return Date.UTC(Number(parts.year), Number(parts.month) - 1, Number(parts.day));
  };
  const difference = day(displayNow()) - day(ms);
  if (difference === 0) return displayTime(ms);
  if (difference === 86_400_000) return locale.startsWith('pt') ? 'Ontem' : 'Yesterday';
  if (difference > 0 && difference < 7 * 86_400_000) return new Intl.DateTimeFormat(locale, { weekday: 'short', ...zone }).format(ms).replaceAll('.', '');
  return new Intl.DateTimeFormat(locale, { day: 'numeric', month: 'short', ...zone }).format(ms);
}
export function displayTime(ms: number): string {
  const locale = typeof document === 'undefined' ? 'pt-BR' : document.documentElement.lang || 'pt-BR';
  return new Intl.DateTimeFormat(locale, { hour: 'numeric', minute: '2-digit', ...(timeZone ? { timeZone } : {}) }).format(ms);
}
