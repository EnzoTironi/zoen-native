/** Free slots from weekly hours, in the business timezone. Pure; mirrors the core's rules. */

export type Service = { id: string; name: string; priceCents: number; minutes: number };
export type Booking = {
  id: string;
  serviceId: string;
  serviceName: string;
  minutes: number;
  priceCents?: number;
  startMs: number;
  endMs: number;
  byId: string;
  byName?: string; // owner, agent and resolvers only
  status: 'pending' | 'confirmed' | 'cancelled';
  note?: string;
  /** Booked at the same moment as someone who got the slot first (kept as cancelled). */
  conflict?: boolean;
  /** Upcoming, but outside the opening hours since they changed. */
  outsideHours?: boolean;
};
export type ScheduleEvent = {
  id: string;
  kind: 'book_request' | 'cancel_request' | 'outside_hours';
  bookingId: string;
  status: 'open' | 'resolved';
  resolution?: string;
  at: number;
  by: string;
};
export type DayHours = [string, string][]; // ["09:00","12:00"]
export type WeeklyHours = Record<string, DayHours>; // "0"=Sun … "6"=Sat
export type Role = 'owner' | 'agent' | 'resolver' | 'client';

export type ScheduleState = {
  title: string;
  business: string;
  accentHex?: string | null;
  timezone: string;
  currency: string;
  services: Service[];
  weeklyHours: WeeklyHours;
  slotMinutes: number;
  horizonDays: number;
  configured: boolean;
  bookings: Booking[];
  events: ScheduleEvent[];
  busy: { startMs: number; endMs: number }[];
  viewer: { id: string; role: Role };
  next: number;
};

export const DEFAULT_ACCENT = '#3D7A28';
const MINUTE = 60_000;
const DAY = 86_400_000;

/** A calendar day in the business timezone. m is 1–12. */
export type Ymd = { y: number; m: number; d: number };

/** Strict HH:MM, like the core. */
export function parseHm(hm: string): number | null {
  const x = /^(\d\d):(\d\d)$/.exec(hm);
  if (!x) return null;
  const h = Number(x[1]), m = Number(x[2]);
  if (h === 24 && m === 0) return 24 * 60;
  return h < 24 && m < 60 ? h * 60 + m : null;
}

const fmtCache = new Map<string, Intl.DateTimeFormat>();
function partsIn(ms: number, tz: string) {
  let f = fmtCache.get(tz);
  if (!f) {
    f = new Intl.DateTimeFormat('en-US', {
      timeZone: tz, hourCycle: 'h23', year: 'numeric', month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit',
    });
    fmtCache.set(tz, f);
  }
  const p: Record<string, number> = {};
  for (const x of f.formatToParts(new Date(ms))) if (x.type !== 'literal') p[x.type] = Number(x.value);
  return { y: p.year, m: p.month, d: p.day, h: p.hour % 24, mi: p.minute };
}

export function todayIn(tz: string, nowMs = Date.now()): Ymd {
  const p = partsIn(nowMs, tz);
  return { y: p.y, m: p.m, d: p.d };
}

export function addDays(day: Ymd, n: number): Ymd {
  const x = new Date(Date.UTC(day.y, day.m - 1, day.d) + n * DAY);
  return { y: x.getUTCFullYear(), m: x.getUTCMonth() + 1, d: x.getUTCDate() };
}

export const weekday = (day: Ymd) => new Date(Date.UTC(day.y, day.m - 1, day.d)).getUTCDay();
export const sameDay = (a: Ymd, b: Ymd) => a.y === b.y && a.m === b.m && a.d === b.d;
export const dayKey = (day: Ymd) => `${day.y}-${day.m}-${day.d}`;

/** The instant of `minutes` past local midnight on `day` in `tz`. */
export function zonedMs(day: Ymd, minutes: number, tz: string): number {
  const wall = Date.UTC(day.y, day.m - 1, day.d, 0, minutes);
  let ms = wall;
  for (let i = 0; i < 3; i++) {
    const p = partsIn(ms, tz);
    const seen = Date.UTC(p.y, p.m - 1, p.d, p.h, p.mi);
    if (seen === wall) break;
    ms += wall - seen;
  }
  return ms;
}

export function freeSlots(opts: {
  day: Ymd;
  service: Service;
  state: Pick<ScheduleState, 'weeklyHours' | 'slotMinutes' | 'timezone' | 'horizonDays' | 'busy'>;
  nowMs?: number;
}): { startMs: number; endMs: number }[] {
  const { day, service, state } = opts;
  const nowMs = opts.nowMs ?? Date.now();
  const windows = state.weeklyHours[String(weekday(day))] ?? [];
  const step = Math.max(5, state.slotMinutes || 30);
  const out: { startMs: number; endMs: number }[] = [];
  for (const [from, to] of windows) {
    const f = parseHm(from), e = parseHm(to);
    if (f == null || e == null) continue;
    for (let t = f; t + service.minutes <= e; t += step) {
      const startMs = zonedMs(day, t, state.timezone);
      const endMs = startMs + service.minutes * MINUTE;
      if (startMs <= nowMs || startMs > nowMs + (state.horizonDays || 60) * DAY) continue;
      if (state.busy.some((b) => b.startMs < endMs && b.endMs > startMs)) continue;
      out.push({ startMs, endMs });
    }
  }
  return out;
}

export function formatMoney(cents: number, currency = 'BRL', locale = 'pt-BR'): string {
  try {
    return new Intl.NumberFormat(locale, { style: 'currency', currency }).format(cents / 100);
  } catch {
    return `${currency} ${(cents / 100).toFixed(2)}`;
  }
}

export function formatTime(ms: number, tz: string, locale = 'pt-BR'): string {
  return new Intl.DateTimeFormat(locale, { hour: '2-digit', minute: '2-digit', timeZone: tz }).format(new Date(ms));
}

export function formatDayLabel(day: Ymd, locale = 'pt-BR', today: Ymd): string {
  const pt = locale.startsWith('pt');
  if (sameDay(day, today)) return pt ? 'Hoje' : 'Today';
  if (sameDay(day, addDays(today, 1))) return pt ? 'Amanhã' : 'Tomorrow';
  return new Intl.DateTimeFormat(locale, { weekday: 'short', day: 'numeric', timeZone: 'UTC' })
    .format(new Date(Date.UTC(day.y, day.m - 1, day.d, 12)));
}

export function weekdayShort(day: Ymd, locale = 'pt-BR'): string {
  return new Intl.DateTimeFormat(locale, { weekday: 'short', timeZone: 'UTC' })
    .format(new Date(Date.UTC(day.y, day.m - 1, day.d, 12)))
    .replace(/[.,]+$/, '');
}

export function dayOf(ms: number, tz: string): Ymd {
  return todayIn(tz, ms);
}
