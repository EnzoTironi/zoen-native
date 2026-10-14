// Agenda: generic booking template. Agent fills services, hours and slot length;
// clients pick a free slot; the booking is an event the agent resolves (edge cases stay with the agent).
import { createRoot } from 'react-dom/client';
import { useCallback, useEffect, useMemo, useState } from 'react';
import './styles.css';
import { zoen, useZoenState, useMembers, GlassCircle, Pill } from '@zoen/ui';
import type { Member } from '@zoen/miniapp-sdk';
import {
  type ScheduleState, type Service, type Booking,
  freeSlots, formatMoney, formatTime, formatDayLabel, tradeAccent,
  startOfLocalDay, addDays, 
} from './slots';
import { L, isPt } from './i18n';

const DEMO: ScheduleState = {
  title: 'Agenda do João',
  business: 'Barbearia do João',
  trade: 'barber',
  timezone: 'America/Sao_Paulo',
  currency: 'BRL',
  services: [
    { id: 'cut', name: 'Corte', priceCents: 4500, minutes: 30 },
    { id: 'beard', name: 'Barba', priceCents: 3500, minutes: 20 },
    { id: 'combo', name: 'Corte + barba', priceCents: 7000, minutes: 50 },
  ],
  weeklyHours: {
    '0': [],
    '1': [['09:00', '12:00'], ['14:00', '19:00']],
    '2': [['09:00', '12:00'], ['14:00', '19:00']],
    '3': [['09:00', '12:00'], ['14:00', '19:00']],
    '4': [['09:00', '12:00'], ['14:00', '19:00']],
    '5': [['09:00', '12:00'], ['14:00', '20:00']],
    '6': [['09:00', '13:00']],
  },
  slotMinutes: 30,
  bookings: [],
  events: [],
  next: 1,
  log: [],
};

const DEMO_MEMBERS: Member[] = [
  { id: 'me', name: 'Enzo', initials: 'E', color: '#4F7BEF', isMe: true },
  { id: 'j', name: 'João', initials: 'J', color: '#C45C26', isMe: false },
];

function useSchedule() {
  const live = useZoenState<ScheduleState>();
  const [local, setLocal] = useState<ScheduleState>(DEMO);
  const hosted = zoen.capabilities.host !== 'none' && zoen.capabilities.host !== 'unknown';
  const state = (hosted && live?.services ? live : local) as ScheduleState;
  const membersLive = useMembers();
  const members = membersLive.length ? membersLive : DEMO_MEMBERS;
  const me = members.find((m) => m.isMe) ?? members[0];
  const call = useCallback(async (tool: string, args: Record<string, any> = {}) => {
    if (hosted) return zoen.state.call(tool, args);
    setLocal((s) => {
      const n: ScheduleState = JSON.parse(JSON.stringify(s));
      if (tool === 'schedule_book') {
        const svc = n.services.find((x) => x.id === args.serviceId)!;
        const id = `b${n.next++}`;
        const booking: Booking = {
          id, serviceId: svc.id, startMs: args.startMs, endMs: args.endMs,
          by: me.name, status: 'pending', note: args.note || '',
        };
        n.bookings.push(booking);
        n.events.push({ id: `e${id}`, kind: 'book_request', bookingId: id, status: 'open', at: Date.now(), by: me.name });
      }
      if (tool === 'schedule_cancel') {
        const b = n.bookings.find((x) => x.id === args.bookingId);
        if (b && b.by === me.name) {
          b.status = 'cancelled';
          n.events.push({ id: `e${n.next++}`, kind: 'cancel_request', bookingId: b.id, status: 'open', at: Date.now(), by: me.name });
        }
      }
      return n;
    });
  }, [hosted, me?.name]);
  return { state, members, me, call, hosted };
}

function App() {
  const { state, me, call, hosted } = useSchedule();
  const locale = isPt() ? 'pt-BR' : (zoen.locale || 'en');
  const accent = tradeAccent(state.trade);
  const [svcId, setSvcId] = useState<string | null>(state.services[0]?.id ?? null);
  const [day, setDay] = useState(() => startOfLocalDay(new Date()));
  const [pick, setPick] = useState<{ startMs: number; endMs: number } | null>(null);
  const [confirm, setConfirm] = useState(false);
  const [success, setSuccess] = useState<Booking | null>(null);
  const [toast, setToast] = useState<string | null>(null);
  const [, setReady] = useState(false);
  useEffect(() => { zoen.ready().then(() => setReady(true)); }, []);
  useEffect(() => {
    document.documentElement.style.setProperty('--accent', accent);
  }, [accent]);

  const service = state.services.find((s) => s.id === svcId) ?? state.services[0];
  const days = useMemo(() => Array.from({ length: 14 }, (_, i) => addDays(startOfLocalDay(new Date()), i)), []);
  const slots = useMemo(() => {
    if (!service) return [];
    return freeSlots({
      day, service, weeklyHours: state.weeklyHours,
      slotMinutes: state.slotMinutes, bookings: state.bookings,
    });
  }, [day, service, state.weeklyHours, state.slotMinutes, state.bookings]);

  const mine = state.bookings
    .filter((b) => b.by === me.name && b.status !== 'cancelled')
    .sort((a, b) => a.startMs - b.startMs);

  const say = (t: string) => { setToast(t); setTimeout(() => setToast(null), 2200); };
  const closed = !(state.weeklyHours[String(day.getDay())]?.length);

  const requestBook = async () => {
    if (!service || !pick) return;
    try {
      await call('schedule_book', {
        serviceId: service.id, startMs: pick.startMs, endMs: pick.endMs, note: '',
      });
      zoen.haptics.success();
      setConfirm(false);
      setSuccess({
        id: 'local', serviceId: service.id, startMs: pick.startMs, endMs: pick.endMs,
        by: me.name, status: 'pending',
      });
      setPick(null);
      say(L('Request sent to the agent'));
    } catch (e: any) {
      zoen.haptics.warning();
      say(e?.message || L('Something went wrong'));
    }
  };

  const cancelMine = async (id: string) => {
    try {
      await call('schedule_cancel', { bookingId: id });
      zoen.haptics.select();
      say(L('Cancel request sent'));
    } catch (e: any) {
      zoen.haptics.warning();
      say(e?.message || L('Something went wrong'));
    }
  };

  const addToCal = async (b: Booking) => {
    const svc = state.services.find((s) => s.id === b.serviceId);
    try {
      const r = await zoen.native.calendar.add({
        title: `${state.business}: ${svc?.name ?? L('Booked')}`,
        startMs: b.startMs, endMs: b.endMs,
        location: state.business,
        notes: state.title,
      });
      say(r.saved ? L('Added to your calendar') : L('Not added'));
    } catch (e: any) {
      say(zoen.capabilities.has('calendar.add') ? e.message : L('Calendar isn’t available in this app host'));
    }
  };

  const mark = (state.business || state.title || '?').trim().charAt(0).toUpperCase();
  const tradeLabel = L(state.trade in { barber: 1, dentist: 1, trainer: 1, custom: 1 } ? state.trade : 'custom');

  return (
    <div className="app" style={{ ['--accent' as any]: accent }}>
      <div className="top">
        {hosted && (
          <GlassCircle label={L('Close')} onClick={() => zoen.close()} style={{ width: 44, height: 44, flex: 'none' }}>
            <svg width="16" height="16" viewBox="0 0 16 16" aria-hidden="true"><path d="M3 6l5 5 5-5" stroke="currentColor" strokeWidth="2.2" fill="none" strokeLinecap="round" strokeLinejoin="round" /></svg>
          </GlassCircle>
        )}
        <div className="head z-glass" role="heading" aria-level={1}>
          <div className="mark" style={{ background: accent }} aria-hidden="true">{mark}</div>
          <div style={{ minWidth: 0 }}>
            <b>{state.business || state.title}</b>
            <span>{tradeLabel} · {state.services.length} {L('services')}</span>
          </div>
        </div>
      </div>

      <div className="scroll">
        <section>
          <h2 className="section-title">{L('Services')}</h2>
          <div className="services" role="radiogroup" aria-label={L('Services')}>
            {state.services.map((s) => (
              <button
                key={s.id}
                className="svc"
                role="radio"
                aria-checked={service?.id === s.id}
                onClick={() => { zoen.haptics.select(); setSvcId(s.id); setPick(null); }}
              >
                <b>{s.name}</b>
                <div className="meta">
                  <span className="price">{formatMoney(s.priceCents, state.currency, locale)}</span>
                  <span className="dur">{s.minutes} {L('min')}</span>
                </div>
              </button>
            ))}
          </div>
        </section>

        <section>
          <h2 className="section-title">{L('Pick a day')}</h2>
          <div className="days" role="radiogroup" aria-label={L('Pick a day')}>
            {days.map((d) => {
              const open = !!(state.weeklyHours[String(d.getDay())]?.length);
              const selected = startOfLocalDay(d).getTime() === day.getTime();
              return (
                <button
                  key={d.toISOString()}
                  className="day"
                  role="radio"
                  aria-checked={selected}
                  aria-disabled={!open}
                  disabled={!open}
                  onClick={() => { zoen.haptics.select(); setDay(startOfLocalDay(d)); setPick(null); }}
                >
                  <span className="wd">{formatDayLabel(d, locale).split(' ')[0].replace(/[.,]+$/, '')}</span>
                  <span className="dn">{d.getDate()}</span>
                </button>
              );
            })}
          </div>
        </section>

        <section>
          <h2 className="section-title">{L('Pick a time')}</h2>
          {!service ? (
            <div className="empty">{L('Choose a service first')}</div>
          ) : closed ? (
            <div className="empty">{L('Closed today')}</div>
          ) : slots.length === 0 ? (
            <div className="empty">{L('No free slots')}</div>
          ) : (
            <div className="slots" role="radiogroup" aria-label={L('Pick a time')}>
              {slots.map((s) => (
                <button
                  key={s.startMs}
                  className="slot"
                  role="radio"
                  aria-checked={pick?.startMs === s.startMs}
                  onClick={() => { zoen.haptics.select(); setPick(s); }}
                >
                  {formatTime(s.startMs, locale)}
                </button>
              ))}
            </div>
          )}
        </section>

        <section>
          <h2 className="section-title">{L('My bookings')}</h2>
          {mine.length === 0 ? (
            <div className="empty">{L('Nothing booked yet')}</div>
          ) : (
            <div className="mine">
              {mine.map((b) => {
                const svc = state.services.find((s) => s.id === b.serviceId);
                return (
                  <div key={b.id} className="mine-row">
                    <div>
                      <b>{svc?.name ?? L('Booked')}</b>
                      <span>
                        {formatDayLabel(new Date(b.startMs), locale)} · {formatTime(b.startMs, locale)}
                        {' · '}
                        <span className={`badge ${b.status}`}>{L(b.status === 'pending' ? 'Pending' : 'Confirmed')}</span>
                      </span>
                    </div>
                    <Pill tone="glass" onClick={() => cancelMine(b.id)}>{L('Cancel booking')}</Pill>
                  </div>
                );
              })}
            </div>
          )}
        </section>
      </div>

      {pick && service && (
        <div className="sticky z-glass">
          <div style={{ padding: '0 8px', minWidth: 0 }}>
            <b style={{ display: 'block', fontSize: 14 }}>{service.name}</b>
            <span style={{ fontSize: 13, color: 'var(--z-ink-2)' }}>
              {formatTime(pick.startMs, locale)} · {formatMoney(service.priceCents, state.currency, locale)}
            </span>
          </div>
          <Pill tone="brand" style={{ background: accent }} onClick={() => { zoen.haptics.tap(); setConfirm(true); }}>
            {L('Send request')}
          </Pill>
        </div>
      )}

      {confirm && service && pick && (
        <div className="sheet" role="dialog" aria-label={L('Confirm')} onClick={() => setConfirm(false)}>
          <div className="sheet-card" onClick={(e) => e.stopPropagation()}>
            <h2>{L('Confirm')}</h2>
            <div className="sheet-row"><span>{L('What')}</span><b>{service.name}</b></div>
            <div className="sheet-row"><span>{L('When')}</span><b>{formatDayLabel(new Date(pick.startMs), locale)} · {formatTime(pick.startMs, locale)}</b></div>
            <div className="sheet-row"><span>{L('Price')}</span><b>{formatMoney(service.priceCents, state.currency, locale)}</b></div>
            <div className="sheet-row"><span>{L('Duration')}</span><b>{service.minutes} {L('min')}</b></div>
            <p style={{ margin: 0, color: 'var(--z-ink-2)', fontSize: 14 }}>{L('Waiting for confirmation')}</p>
            <div className="sheet-actions">
              <Pill tone="glass" onClick={() => setConfirm(false)}>{L('Back')}</Pill>
              <Pill tone="brand" style={{ background: accent }} onClick={requestBook}>{L('Send request')}</Pill>
            </div>
          </div>
        </div>
      )}

      {success && (
        <div className="success" role="status">
          <div className="success-card">
            <div className="stamp" aria-hidden="true">
              <svg width="32" height="32" viewBox="0 0 32 32"><path d="M7 17l6 6 12-14" stroke="currentColor" strokeWidth="3.2" fill="none" strokeLinecap="round" strokeLinejoin="round" /></svg>
            </div>
            <h2>{L('Ask sent')}</h2>
            <p>{L('Request sent to the agent')}</p>
            <div className="sheet-actions">
              <Pill tone="glass" onClick={() => addToCal(success)}>{L('Add to calendar')}</Pill>
              <Pill tone="brand" style={{ background: accent }} onClick={() => setSuccess(null)}>{L('Close')}</Pill>
            </div>
          </div>
        </div>
      )}

      {toast && <div className="toast z-glass" role="status">{toast}</div>}
    </div>
  );
}

// Silence unused import in production builds that tree-shake demo helpers.

zoen.ready();
createRoot(document.getElementById('root')!).render(<App />);
