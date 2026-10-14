/** Free slots from weekly hours, slot length and booked times. Pure; the agent owns edge cases. */

export type Service = { id: string; name: string; priceCents: number; minutes: number };
export type Booking = {
  id: string;
  serviceId: string;
  startMs: number;
  endMs: number;
  by: string;
  status: 'pending' | 'confirmed' | 'cancelled';
  note?: string;
};
export type DayHours = [string, string][]; // "09:00","12:00"
export type WeeklyHours = Record<string, DayHours>; // "0"=Sun … "6"=Sat

export type ScheduleState = {
  title: string;
  business: string;
  trade: 'barber' | 'dentist' | 'trainer' | 'custom' | string;
  timezone: string;
  currency: string;
  services: Service[];
  weeklyHours: WeeklyHours;
  slotMinutes: number;
  bookings: Booking[];
  events: { id: string; kind: string; bookingId: string; status: string; at: number; by: string }[];
  next: number;
  log: { who: string; what: string; at: number }[];
};

export function parseHm(hm: string): number {
  const [h, m] = hm.split(':').map(Number);
  return (h || 0) * 60 + (m || 0);
}

export function dayKey(d: Date): string {
  return String(d.getDay()); // local day; host locale drives display
}

export function startOfLocalDay(d: Date): Date {
  const x = new Date(d);
  x.setHours(0, 0, 0, 0);
  return x;
}

export function addDays(d: Date, n: number): Date {
  const x = new Date(d);
  x.setDate(x.getDate() + n);
  return x;
}

/** Occupying bookings block a slot from the client's view. Agent can still resolve overlaps. */
export function occupies(b: Booking): boolean {
  return b.status === 'pending' || b.status === 'confirmed';
}

export function freeSlots(opts: {
  day: Date;
  service: Service;
  weeklyHours: WeeklyHours;
  slotMinutes: number;
  bookings: Booking[];
  nowMs?: number;
}): { startMs: number; endMs: number }[] {
  const { day, service, weeklyHours, slotMinutes, bookings } = opts;
  const nowMs = opts.nowMs ?? Date.now();
  const windows = weeklyHours[dayKey(day)] ?? [];
  if (!windows.length) return [];
  const step = Math.max(5, slotMinutes || service.minutes || 30);
  const duration = Math.max(step, service.minutes || step);
  const base = startOfLocalDay(day).getTime();
  const taken = bookings.filter(occupies);
  const out: { startMs: number; endMs: number }[] = [];
  for (const [from, to] of windows) {
    let t = parseHm(from);
    const end = parseHm(to);
    while (t + duration <= end) {
      const startMs = base + t * 60_000;
      const endMs = startMs + duration * 60_000;
      if (startMs >= nowMs - 60_000) {
        const clash = taken.some((b) => b.startMs < endMs && b.endMs > startMs);
        if (!clash) out.push({ startMs, endMs });
      }
      t += step;
    }
  }
  return out;
}

export function formatMoney(cents: number, currency = 'BRL', locale = 'pt-BR'): string {
  try {
    return new Intl.NumberFormat(locale, { style: 'currency', currency }).format(cents / 100);
  } catch {
    return `R$ ${(cents / 100).toFixed(2)}`;
  }
}

export function formatTime(ms: number, locale = 'pt-BR'): string {
  return new Intl.DateTimeFormat(locale, { hour: '2-digit', minute: '2-digit' }).format(new Date(ms));
}

export function formatDayLabel(d: Date, locale = 'pt-BR', today: Date = new Date()): string {
  const a = startOfLocalDay(d).getTime();
  const b = startOfLocalDay(today).getTime();
  if (a === b) return locale.startsWith('pt') ? 'Hoje' : 'Today';
  if (a === b + 86_400_000) return locale.startsWith('pt') ? 'Amanhã' : 'Tomorrow';
  return new Intl.DateTimeFormat(locale, { weekday: 'short', day: 'numeric' }).format(d);
}

export function tradeAccent(trade: string): string {
  switch (trade) {
    case 'dentist': return '#3D6FBF';
    case 'trainer': return '#0F9B8E';
    case 'barber': return '#C45C26';
    default: return '#3D7A28';
  }
}
