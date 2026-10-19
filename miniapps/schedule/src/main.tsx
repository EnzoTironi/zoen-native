// Agenda: a booking template for any business. The agent fills the name, services and
// hours from the owner's words; clients pick a free slot; each request is an event the
// agent resolves. The core filters what each person sees: clients get busy times only.
import { createRoot } from 'react-dom/client';
import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  useSyncExternalStore,
} from 'react';
import './styles.css';
import { zoen, useZoenState, useMembers, GlassCircle, Pill } from '@zoen/ui';
import {
  type ScheduleState,
  type Booking,
  type ScheduleEvent,
  type Ymd,
  DEFAULT_ACCENT,
  freeSlots,
  formatMoney,
  formatTime,
  formatDayLabel,
  weekdayShort,
  todayIn,
  addDays,
  weekday,
  sameDay,
  dayKey,
  dayOf,
} from './slots';
import { L, isPt } from './i18n';
import { availabilityScope, conflicts, createAvailability } from './availability';

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
const isFor = (b: Booking, id?: string) => !!id && (b.byId === id || b.forId === id);

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

function useSchedule() {
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

const openCancel = (events: ScheduleEvent[], b: Booking) =>
  events.some((e) => e.bookingId === b.id && e.kind === 'cancel_request' && e.status === 'open');

function App() {
  const { state, call, hosted } = useSchedule();
  const members = useMembers();
  const locale = isPt() ? 'pt-BR' : zoen.locale || 'en';
  const tz = state.timezone || 'America/Sao_Paulo';
  const accent = state.accentHex || DEFAULT_ACCENT;
  const staff = state.viewer?.role && state.viewer.role !== 'client';
  const today = todayIn(tz);
  const [svcId, setSvcId] = useState<string | null>(null);
  const [day, setDay] = useState<Ymd>(today);
  const [pick, setPick] = useState<{ startMs: number; endMs: number } | null>(null);
  const [confirm, setConfirm] = useState(false);
  const [sent, setSent] = useState(false);
  const [toast, setToast] = useState<string | null>(null);
  const [canCheckCalendar, setCanCheckCalendar] = useState(() =>
    zoen.capabilities.has('calendar.freebusy'),
  );
  useEffect(() => {
    let live = true;
    void zoen.ready().then(() => {
      if (live) setCanCheckCalendar(zoen.capabilities.has('calendar.freebusy'));
    });
    return () => {
      live = false;
    };
  }, []);

  const service = state.services.find((s) => s.id === svcId) ?? state.services[0];
  const horizon = Math.min(state.horizonDays || 60, 21);
  const days = useMemo(
    () => Array.from({ length: horizon }, (_, i) => addDays(today, i)),
    [dayKey(today), horizon],
  );
  const slots = useMemo(
    () => (service ? freeSlots({ day, service, state }) : []),
    [dayKey(day), service, state.weeklyHours, state.slotMinutes, state.busy, tz, state.horizonDays],
  );
  const scope = useMemo(
    () => availabilityScope(day, tz, state.viewer?.id || ''),
    [dayKey(day), tz, state.viewer?.id],
  );
  const [calendar] = useState(() =>
    createAvailability((startMs, endMs) => zoen.native.calendar.freeBusy(startMs, endMs)),
  );
  const calendarState = useSyncExternalStore(calendar.subscribe, calendar.snapshot);
  const availability =
    calendarState.scope === scope.key ? calendarState : { phase: 'idle' as const };
  const personalBusy = availability.phase === 'ready' ? availability.busy : [];
  const allConflict =
    slots.length > 0 &&
    availability.phase === 'ready' &&
    slots.every((slot) => conflicts(slot, personalBusy));
  const selection = useRef({ scope, service, pick });
  selection.current = { scope, service, pick };
  const clearSelection = () => {
    selection.current = { ...selection.current, pick: null };
    setPick(null);
    setConfirm(false);
  };
  useLayoutEffect(() => {
    calendar.setScope(scope);
    clearSelection();
  }, [calendar, scope]);
  useEffect(() => () => calendar.dispose(), [calendar]);
  useEffect(() => {
    if (pick && !calendar.canBook(scope, pick)) clearSelection();
  }, [calendar, calendarState, scope, pick]);
  const mine = state.bookings
    .filter((b) => isFor(b, state.viewer?.id) && b.status !== 'cancelled')
    .sort((a, b) => a.startMs - b.startMs);
  // Booked at the same moment as someone else, who got the slot first.
  const taken = state.bookings.filter(
    (b) => isFor(b, state.viewer?.id) && b.conflict && b.endMs > Date.now(),
  );
  const requests = staff ? state.events.filter((e) => e.status === 'open') : [];
  const closed = !state.weeklyHours[String(weekday(day))]?.length;

  const say = (t: string) => {
    setToast(t);
    setTimeout(() => setToast(null), 2200);
  };
  const fail = (e: any) => {
    zoen.haptics.warning();
    say(e?.message || L('Something went wrong'));
  };
  const when = (ms: number) =>
    `${formatDayLabel(dayOf(ms, tz), locale, today)} · ${formatTime(ms, tz, locale)}`;
  const nameOf = (b: Booking) =>
    b.forName || b.byName || members.find((m) => m.id === b.byId)?.name || '';

  const requestBook = async () => {
    if (!service || !pick) return;
    if (
      selection.current.scope.key !== scope.key ||
      selection.current.service?.id !== service.id ||
      selection.current.pick !== pick ||
      !calendar.canBook(scope, pick)
    ) {
      clearSelection();
      return;
    }
    try {
      await call('schedule_book', { serviceId: service.id, startMs: pick.startMs });
      zoen.haptics.success();
      setConfirm(false);
      setPick(null);
      setSent(true);
    } catch (e) {
      fail(e);
    }
  };
  const cancelMine = async (b: Booking) => {
    try {
      await call('schedule_cancel', { bookingId: b.id });
      zoen.haptics.select();
      say(L(b.status === 'pending' ? 'Request cancelled' : 'Cancel request sent'));
    } catch (e) {
      fail(e);
    }
  };
  const resolve = async (bookingId: string, action: 'confirm' | 'cancel' | 'keep') => {
    try {
      await call('schedule_resolve', { bookingId, action });
      zoen.haptics.success();
    } catch (e) {
      fail(e);
    }
  };
  const addToCal = async (b: Booking) => {
    try {
      const r = await zoen.native.calendar.add({
        title: `${state.business}: ${b.serviceName}`,
        startMs: b.startMs,
        endMs: b.endMs,
        location: state.business,
      });
      say(r.saved ? L('Added to your calendar') : L('Not added'));
    } catch (e: any) {
      say(
        zoen.capabilities.has('calendar.add')
          ? e.message
          : L('Calendar isn’t available in this app host'),
      );
    }
  };

  const mark = (state.business || state.title || '?').trim().charAt(0).toUpperCase();

  return (
    <div className="app" style={{ ['--accent' as any]: accent }}>
      <div className="top">
        {hosted && (
          <GlassCircle
            label={L('Close')}
            onClick={() => zoen.close()}
            style={{ width: 44, height: 44, flex: 'none' }}
          >
            <svg width="16" height="16" viewBox="0 0 16 16" aria-hidden="true">
              <path
                d="M3 6l5 5 5-5"
                stroke="currentColor"
                strokeWidth="2.2"
                fill="none"
                strokeLinecap="round"
                strokeLinejoin="round"
              />
            </svg>
          </GlassCircle>
        )}
        <div className="head z-glass" role="heading" aria-level={1}>
          <div className="mark" style={{ background: accent }} aria-hidden="true">
            {mark}
          </div>
          <div style={{ minWidth: 0 }}>
            <b>{state.business || state.title}</b>
            <span>
              {state.services.length} {L('services')}
            </span>
          </div>
        </div>
      </div>

      <div className="scroll">
        {!state.configured ? (
          <div className="empty setup">{L('The agent is setting up this schedule')}</div>
        ) : (
          <>
            {staff && (
              <section>
                <h2 className="section-title">{L('Requests')}</h2>
                {requests.length === 0 ? (
                  <div className="empty">{L('No open requests')}</div>
                ) : (
                  <div className="mine">
                    {requests.map((ev) => {
                      const b = state.bookings.find((x) => x.id === ev.bookingId);
                      if (!b) return null;
                      // A cancel request and an hours flag both end in keep or cancel.
                      const cancelAsk = ev.kind === 'cancel_request';
                      const outside = ev.kind === 'outside_hours';
                      const keepOrCancel = cancelAsk || outside;
                      return (
                        <div key={ev.id} className="mine-row">
                          <div>
                            <b>
                              {b.serviceName}
                              {nameOf(b) && ` · ${nameOf(b)}`}
                            </b>
                            <span>
                              {when(b.startMs)}
                              {cancelAsk && ` · ${L('Wants to cancel')}`}
                              {outside && ` · ${L('Outside the new hours')}`}
                            </span>
                          </div>
                          <div className="row-actions">
                            <Pill
                              tone="glass"
                              onClick={() => resolve(b.id, keepOrCancel ? 'keep' : 'cancel')}
                            >
                              {L(keepOrCancel ? 'Keep' : 'Decline')}
                            </Pill>
                            <Pill
                              tone="brand"
                              style={{ background: accent }}
                              onClick={() => resolve(b.id, keepOrCancel ? 'cancel' : 'confirm')}
                            >
                              {L(keepOrCancel ? 'Cancel booking' : 'Confirm')}
                            </Pill>
                          </div>
                        </div>
                      );
                    })}
                  </div>
                )}
              </section>
            )}

            <section>
              <h2 className="section-title">{L('Services')}</h2>
              <div className="services" role="radiogroup" aria-label={L('Services')}>
                {state.services.map((s) => (
                  <button
                    key={s.id}
                    className="svc"
                    role="radio"
                    aria-checked={service?.id === s.id}
                    onClick={() => {
                      zoen.haptics.select();
                      clearSelection();
                      setSvcId(s.id);
                    }}
                  >
                    <b>{s.name}</b>
                    <div className="meta">
                      <span className="price">
                        {formatMoney(s.priceCents, state.currency, locale)}
                      </span>
                      <span className="dur">
                        {s.minutes} {L('min')}
                      </span>
                    </div>
                  </button>
                ))}
              </div>
            </section>

            <section>
              <h2 className="section-title">{L('Pick a day')}</h2>
              <div className="days" role="radiogroup" aria-label={L('Pick a day')}>
                {days.map((d) => {
                  const open = !!state.weeklyHours[String(weekday(d))]?.length;
                  return (
                    <button
                      key={dayKey(d)}
                      className="day"
                      role="radio"
                      aria-checked={sameDay(d, day)}
                      aria-disabled={!open}
                      disabled={!open}
                      onClick={() => {
                        zoen.haptics.select();
                        clearSelection();
                        calendar.setScope(availabilityScope(d, tz, state.viewer?.id || ''));
                        setDay(d);
                      }}
                    >
                      <span className="wd">
                        {sameDay(d, today) ? L('Today') : weekdayShort(d, locale)}
                      </span>
                      <span className="dn">{d.d}</span>
                    </button>
                  );
                })}
              </div>
            </section>

            <section>
              <h2 className="section-title">{L('Pick a time')}</h2>
              {canCheckCalendar && (
                <div className="availability">
                  <b>{L('My availability')}</b>
                  <p>
                    {L(
                      'Check this device for events marked busy. Only time ranges are used here; event names and details are not shared. Results stay private until you close this schedule.',
                    )}
                  </p>
                  <p className="availability-day">
                    {formatDayLabel(day, locale, today)} · {tz}
                  </p>
                  <div role="status" aria-live="polite">
                    {availability.phase === 'loading' && <p>{L('Checking your calendar…')}</p>}
                    {availability.phase === 'ready' && (
                      <p>
                        {L(
                          slots.some((slot) => conflicts(slot, personalBusy))
                            ? 'Calendar conflicts are marked below.'
                            : 'None of these business times conflict with events marked busy.',
                        )}
                      </p>
                    )}
                    {availability.phase === 'error' && (
                      <p>
                        {L('Your calendar was not checked. No filter is applied.')}{' '}
                        {L(availability.message)}
                      </p>
                    )}
                  </div>
                  <div className="availability-actions">
                    {availability.phase !== 'loading' && (
                      <Pill
                        tone="glass"
                        onClick={() => {
                          clearSelection();
                          void calendar.check(scope);
                        }}
                      >
                        {L(
                          availability.phase === 'error'
                            ? 'Try again'
                            : availability.phase === 'ready'
                              ? 'Check again'
                              : 'Check my availability',
                        )}
                      </Pill>
                    )}
                    {availability.phase !== 'idle' && (
                      <Pill tone="glass" onClick={() => calendar.clear(scope)}>
                        {L(
                          availability.phase === 'loading'
                            ? 'Cancel check'
                            : 'Remove calendar check',
                        )}
                      </Pill>
                    )}
                  </div>
                </div>
              )}
              {!service ? (
                <div className="empty">{L('Choose a service first')}</div>
              ) : closed ? (
                <div className="empty">{L('Closed today')}</div>
              ) : slots.length === 0 ? (
                <div className="empty">{L('No free slots')}</div>
              ) : (
                <>
                  {allConflict && (
                    <div className="empty">
                      {L(
                        'Business times are available, but all conflict with your calendar. Remove the calendar check to choose any business time.',
                      )}
                    </div>
                  )}
                  <div className="slots" role="radiogroup" aria-label={L('Pick a time')}>
                    {slots.map((s) => {
                      const busy = conflicts(s, personalBusy);
                      return (
                        <button
                          key={s.startMs}
                          className="slot"
                          role="radio"
                          aria-checked={pick?.startMs === s.startMs}
                          disabled={busy || availability.phase === 'loading'}
                          data-calendar-busy={busy || undefined}
                          onClick={() => {
                            if (!calendar.canBook(scope, s)) return;
                            zoen.haptics.select();
                            setPick(s);
                          }}
                        >
                          {formatTime(s.startMs, tz, locale)}
                          {busy && <small>{L('Busy in my calendar')}</small>}
                        </button>
                      );
                    })}
                  </div>
                </>
              )}
            </section>

            <section>
              <h2 className="section-title">{L('My bookings')}</h2>
              {taken.map((b) => (
                <div key={b.id} className="empty" role="status">
                  {L('Someone booked this time just before you')}
                  {`: ${b.serviceName}, ${when(b.startMs)}. `}
                  {L('Pick another time.')}
                </div>
              ))}
              {mine.length === 0 ? (
                taken.length === 0 && <div className="empty">{L('Nothing booked yet')}</div>
              ) : (
                <div className="mine">
                  {mine.map((b) => {
                    const asked = openCancel(state.events, b);
                    const badge = asked
                      ? 'Cancel requested'
                      : b.status === 'pending'
                        ? 'Pending'
                        : 'Confirmed';
                    return (
                      <div key={b.id} className="mine-row">
                        <div>
                          <b>{b.serviceName}</b>
                          <span>
                            {when(b.startMs)}
                            {' · '}
                            <span className={`badge ${asked ? 'pending' : b.status}`}>
                              {L(badge)}
                            </span>
                          </span>
                        </div>
                        <div className="row-actions">
                          {b.status === 'confirmed' && !asked && (
                            <Pill tone="glass" onClick={() => addToCal(b)}>
                              {L('Add to calendar')}
                            </Pill>
                          )}
                          {!asked && (
                            <Pill tone="glass" onClick={() => cancelMine(b)}>
                              {L('Cancel booking')}
                            </Pill>
                          )}
                        </div>
                      </div>
                    );
                  })}
                </div>
              )}
            </section>
          </>
        )}
      </div>

      {pick && service && (
        <div className="sticky z-glass">
          <div style={{ padding: '0 8px', minWidth: 0 }}>
            <b style={{ display: 'block', fontSize: 14 }}>{service.name}</b>
            <span style={{ fontSize: 13, color: 'var(--z-ink-2)' }}>
              {formatTime(pick.startMs, tz, locale)} ·{' '}
              {formatMoney(service.priceCents, state.currency, locale)}
            </span>
          </div>
          <Pill
            tone="brand"
            style={{ background: accent }}
            onClick={() => {
              zoen.haptics.tap();
              setConfirm(true);
            }}
          >
            {L('Send request')}
          </Pill>
        </div>
      )}

      {confirm && service && pick && (
        <div
          className="sheet"
          role="dialog"
          aria-label={L('Confirm')}
          onClick={() => setConfirm(false)}
        >
          <div className="sheet-card" onClick={(e) => e.stopPropagation()}>
            <h2>{L('Confirm')}</h2>
            <div className="sheet-row">
              <span>{L('What')}</span>
              <b>{service.name}</b>
            </div>
            <div className="sheet-row">
              <span>{L('When')}</span>
              <b>{when(pick.startMs)}</b>
            </div>
            <div className="sheet-row">
              <span>{L('Price')}</span>
              <b>{formatMoney(service.priceCents, state.currency, locale)}</b>
            </div>
            <div className="sheet-row">
              <span>{L('Duration')}</span>
              <b>
                {service.minutes} {L('min')}
              </b>
            </div>
            <p style={{ margin: 0, color: 'var(--z-ink-2)', fontSize: 14 }}>
              {L('Waiting for confirmation')}
            </p>
            <div className="sheet-actions">
              <Pill tone="glass" onClick={() => setConfirm(false)}>
                {L('Back')}
              </Pill>
              <Pill tone="brand" style={{ background: accent }} onClick={requestBook}>
                {L('Send request')}
              </Pill>
            </div>
          </div>
        </div>
      )}

      {sent && (
        <div className="success" role="status">
          <div className="success-card">
            <div className="stamp" aria-hidden="true">
              <svg width="32" height="32" viewBox="0 0 32 32">
                <path
                  d="M7 17l6 6 12-14"
                  stroke="currentColor"
                  strokeWidth="3.2"
                  fill="none"
                  strokeLinecap="round"
                  strokeLinejoin="round"
                />
              </svg>
            </div>
            <h2>{L('Ask sent')}</h2>
            <p>{L('You can add it to your calendar once it’s confirmed')}</p>
            <div className="sheet-actions">
              <Pill tone="brand" style={{ background: accent }} onClick={() => setSent(false)}>
                {L('Close')}
              </Pill>
            </div>
          </div>
        </div>
      )}

      {toast && (
        <div className="toast z-glass" role="status">
          {toast}
        </div>
      )}
    </div>
  );
}

createRoot(document.getElementById('root')!).render(<App />);
