import { useCallback, useState } from 'react';
import { zoen, useZoenState } from '@zoen/ui';
import type { ScheduleState, Booking, ScheduleEvent } from './slots';
import { L } from './i18n';

const DEMO: ScheduleState = {
  title: 'Studio',
  business: 'Studio',
  accentHex: null,
  timezone: 'America/Sao_Paulo',
  currency: 'BRL',
  services: [
    { id: 'session', name: 'Sessão', priceCents: 9000, minutes: 50 },
    { id: 'quick', name: 'Retorno', priceCents: 4000, minutes: 30 },
  ],
  weeklyHours: {
    '0': [],
    '1': [
      ['09:00', '12:00'],
      ['14:00', '19:00'],
    ],
    '2': [
      ['09:00', '12:00'],
      ['14:00', '19:00'],
    ],
    '3': [
      ['09:00', '12:00'],
      ['14:00', '19:00'],
    ],
    '4': [
      ['09:00', '12:00'],
      ['14:00', '19:00'],
    ],
    '5': [
      ['09:00', '12:00'],
      ['14:00', '19:00'],
    ],
    '6': [['09:00', '13:00']],
  },
  slotMinutes: 30,
  horizonDays: 60,
  configured: true,
  bookings: [],
  events: [],
  busy: [],
  viewer: { id: 'me', role: 'client' },
  next: 1,
};

/** Theirs when they made it or the agent booked it in their name (the core's `is_for`). */
export const isFor = (b: Booking, id?: string) => !!id && (b.byId === id || b.forId === id);

/** Offline preview only: mirrors the core's book/cancel so the template can be tried. */
function demoCall(s: ScheduleState, tool: string, args: Record<string, any>): ScheduleState {
  const n: ScheduleState = structuredClone(s);
  const now = Date.now();
  if (tool === 'schedule_book') {
    const svc = n.services.find((x) => x.id === args.serviceId);
    if (!svc) throw new Error(L('Something went wrong'));
    const id = `b${n.next++}`;
    n.bookings.push({
      id,
      serviceId: svc.id,
      serviceName: svc.name,
      minutes: svc.minutes,
      startMs: args.startMs,
      endMs: args.startMs + svc.minutes * 60_000,
      byId: n.viewer.id,
      status: 'pending',
    });
    n.events.push({
      id: `e${n.next++}`,
      kind: 'book_request',
      bookingId: id,
      status: 'open',
      at: now,
      by: n.viewer.id,
    });
  }
  if (tool === 'schedule_cancel') {
    const b = n.bookings.find((x) => x.id === args.bookingId && isFor(x, n.viewer.id));
    if (!b || b.status === 'cancelled') throw new Error(L('Something went wrong'));
    if (b.status === 'pending') {
      b.status = 'cancelled';
      for (const e of n.events)
        if (e.bookingId === b.id && e.status === 'open') {
          e.status = 'resolved';
          e.resolution = 'withdrawn';
        }
    } else {
      n.events.push({
        id: `e${n.next++}`,
        kind: 'cancel_request',
        bookingId: b.id,
        status: 'open',
        at: now,
        by: n.viewer.id,
      });
    }
  }
  n.busy = n.bookings
    .filter((b) => b.status !== 'cancelled')
    .map(({ startMs, endMs }) => ({ startMs, endMs }));
  return n;
}

export function useSchedule() {
  const live = useZoenState<ScheduleState>();
  const [local, setLocal] = useState<ScheduleState>(DEMO);
  const hosted = zoen.capabilities.host !== 'none' && zoen.capabilities.host !== 'unknown';
  const state = (hosted && live?.services ? live : local) as ScheduleState;
  const call = useCallback(
    async (tool: string, args: Record<string, any> = {}) => {
      if (hosted) return zoen.state.call(tool, args);
      setLocal((s) => demoCall(s, tool, args));
    },
    [hosted],
  );
  return { state, call, hosted };
}

export const openCancel = (events: ScheduleEvent[], b: Booking) =>
  events.some((e) => e.bookingId === b.id && e.kind === 'cancel_request' && e.status === 'open');

