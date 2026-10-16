let fixed: number | undefined;
let timeZone: string | undefined;
export function displayNow(): number { return fixed ?? Date.now(); }
export function isDisplayClockFixed(): boolean { return fixed !== undefined; }
export function configureDisplayClock(config?: { now: number; timeZone: string }) { fixed = config?.now; timeZone = config?.timeZone; }
export function displayTime(ms: number): string {
  const locale = typeof document === 'undefined' ? 'pt-BR' : document.documentElement.lang || 'pt-BR';
  return new Intl.DateTimeFormat(locale, { hour: 'numeric', minute: '2-digit', ...(timeZone ? { timeZone } : {}) }).format(ms);
}
