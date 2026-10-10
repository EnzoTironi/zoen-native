// Saturday hike: the hero mini-app. Map with the three routes, a carousel that drives the
// camera, a detail page per trail, a compare table and group voting, all on shared state.
import { createRoot } from 'react-dom/client';
import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';
import maplibregl from 'maplibre-gl';
import 'maplibre-gl/dist/maplibre-gl.css';
import './styles.css';
import { zoen, useZoenState, useMembers, Carousel, GlassCircle, GlassPill, Pill, Tag, Stat, StatGrid, AvatarStack, spring, lerp } from '@zoen/ui';
import type { Member, PickedPhoto } from '@zoen/miniapp-sdk';
import { TRAILS, byId, bounds, haversineMi, type Trail, type TrailId } from './trails';
import { Photo } from './photos';
import { L } from './i18n';
import { OfflineMap } from './OfflineMap';

interface HikeState {
  title: string; area: string; day: string; dayMs?: number;
  trails: { id: TrailId; votes: string[] }[];
  decided: TrailId | null;
  itinerary: { time: string; text: string }[] | null;
  album: { by: string; dataUrl: string }[];
}

// Outside a host (a browser tab, the headless test) the app runs on this local demo state.
const DEMO: HikeState = {
  title: 'Saturday hike', area: 'Bay Area', day: 'Saturday',
  trails: [{ id: 'tomales', votes: ['Marina'] }, { id: 'steep', votes: ['Lucas'] }, { id: 'lands', votes: [] }],
  decided: null, itinerary: null, album: [],
};
const DEMO_MEMBERS: Member[] = [
  { id: 'me', name: 'Enzo', initials: 'E', color: '#4F7BEF', isMe: true },
  { id: 'm', name: 'Marina', initials: 'M', color: '#E0567A', isMe: false },
  { id: 'l', name: 'Lucas', initials: 'L', color: '#2FA37C', isMe: false },
  { id: 'a', name: 'Ana', initials: 'A', color: '#F0A030', isMe: false },
];

type Filter = 'all' | 'shady' | 'short' | 'ocean';
const FILTERS: { id: Filter; label: string; test: (t: Trail) => boolean }[] = [
  { id: 'all', label: 'All hikes', test: () => true },
  { id: 'shady', label: 'Shady', test: (t) => t.shade !== 'None' },
  { id: 'short', label: 'Under 5 mi', test: (t) => t.miles < 5 },
  { id: 'ocean', label: 'Ocean views', test: (t) => /ocean|bridge|city/i.test(t.bestFor + t.attributes.join(' ')) },
];

const ONLINE_STYLE = 'https://tiles.openfreemap.org/styles/liberty';

/** Critically damped spring as an easing curve (no overshoot on the camera). */
const springEase = (t: number) => { const k = 7; return (1 - (1 + k * t) * Math.exp(-k * t)) / (1 - (1 + k) * Math.exp(-k)); };

function useHike() {
  const live = useZoenState<HikeState>();
  const [local, setLocal] = useState<HikeState>(DEMO);
  const hosted = zoen.capabilities.host !== 'none' && zoen.capabilities.host !== 'unknown';
  const state = (hosted && live?.trails ? live : local) as HikeState;
  const membersLive = useMembers();
  const members = membersLive.length ? membersLive : DEMO_MEMBERS;
  const me = members.find((m) => m.isMe) ?? members[0];
  const call = useCallback(async (tool: string, args: Record<string, any> = {}) => {
    if (hosted) return zoen.state.call(tool, args);
    // Local demo: mimic the core.
    setLocal((s) => {
      const n: HikeState = JSON.parse(JSON.stringify(s));
      if (tool === 'hike_vote') { n.trails.forEach((t) => (t.votes = t.votes.filter((v) => v !== me.name))); n.trails.find((t) => t.id === args.trail)!.votes.push(me.name); }
      if (tool === 'hike_decide') { n.decided = [...n.trails].sort((a, b) => b.votes.length - a.votes.length)[0].id; }
      return n;
    });
  }, [hosted, me?.name]);
  return { state, members, me, call, hosted };
}

function votersOf(state: HikeState, id: TrailId, members: Member[]) {
  const names = state.trails.find((t) => t.id === id)?.votes ?? [];
  return names.map((n) => members.find((m) => m.name === n) ?? { id: n, name: n, initials: n.slice(0, 1).toUpperCase(), color: '#8C95A8', isMe: false });
}

function App() {
  const { state, members, me, call, hosted } = useHike();
  const [filter, setFilter] = useState<Filter>('all');
  const [menu, setMenu] = useState(false);
  const [index, setIndex] = useState(0);
  const [detail, setDetail] = useState<{ id: TrailId; from: DOMRect } | null>(null);
  const [compare, setCompare] = useState(false);
  const [here, setHere] = useState<[number, number] | null>(null);
  const [toast, setToast] = useState<string | null>(null);
  const [, setReady] = useState(false);
  useEffect(() => { zoen.ready().then(() => setReady(true)); }, []);
  const shown = useMemo(() => {
    const list = TRAILS.filter(FILTERS.find((f) => f.id === filter)!.test);
    // The chosen trail leads once the group decided.
    return state.decided ? [...list].sort((a, b) => (a.id === state.decided ? -1 : b.id === state.decided ? 1 : 0)) : list;
  }, [filter, state.decided]);
  const current = shown[Math.min(index, shown.length - 1)] ?? TRAILS[0];
  const total = state.trails.reduce((n, t) => n + t.votes.length, 0);
  const leader = [...state.trails].sort((a, b) => b.votes.length - a.votes.length)[0];

  const say = (t: string) => { setToast(t); setTimeout(() => setToast(null), 2200); };

  const vote = async (id: TrailId) => {
    try { await call('hike_vote', { trail: id }); zoen.haptics.success(); say(`${L('Voted for')} ${byId(id).name}`); }
    catch (e: any) { zoen.haptics.warning(); say(e.message); }
  };

  const locate = async () => {
    try { const p = await zoen.native.location.current({ approximate: true }); setHere([p.lon, p.lat]); zoen.haptics.success(); }
    catch (e: any) { say(zoen.capabilities.has('location.approximate') ? e.message : L('Location isn’t available in this app host')); }
  };

  return (
    <>
      <MapView trail={current} />
      <div className="top">
        <div className="lead">
        {hosted && (
          <GlassCircle label={L('Close')} onClick={() => zoen.close()} style={{ width: 44, height: 44, flex: 'none' }}>
            <svg width="16" height="16" viewBox="0 0 16 16" aria-hidden="true"><path d="M3 6l5 5 5-5" stroke="currentColor" strokeWidth="2.2" fill="none" strokeLinecap="round" strokeLinejoin="round" /></svg>
          </GlassCircle>
        )}
        <div className="head z-glass" role="heading" aria-level={1}>
          <div className="thumb"><Photo trail={state.decided ?? 'tomales'} /></div>
          <div style={{ minWidth: 0 }}>
            <b>{state.title}</b>
            <span>{state.decided ? `${byId(state.decided).name} • ${state.day}` : `${TRAILS.length} ${L('options')} • ${state.area}`}</span>
          </div>
        </div>
        </div>
        <div className="right" style={{ position: 'relative' }}>
          <Pill tone="black" onClick={() => setCompare(true)}>{L('Compare')}</Pill>
          <GlassPill onClick={() => setMenu((m) => !m)} label={L('Filter hikes')}>
            {L(FILTERS.find((f) => f.id === filter)!.label)}
            <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true"><path d="M1.5 3.5 5 7l3.5-3.5" stroke="currentColor" strokeWidth="1.8" fill="none" strokeLinecap="round" /></svg>
          </GlassPill>
          {menu && (
            <div className="menu z-glass" role="menu">
              {FILTERS.map((f) => (
                <button key={f.id} role="menuitemradio" aria-checked={f.id === filter} onClick={() => { zoen.haptics.select(); setFilter(f.id); setIndex(0); setMenu(false); }}>
                  {L(f.label)}<span>{TRAILS.filter(f.test).length}</span>
                </button>
              ))}
            </div>
          )}
        </div>
      </div>

      <div className="bottom">
        {!state.decided && total >= 2 && (
          <div className="lockin"><Pill tone="brand" onClick={async () => { await call('hike_decide'); zoen.haptics.success(); }}>{L('Lock in')} {byId(leader.id).name}</Pill></div>
        )}
        <Carousel onIndex={setIndex} key={filter + (state.decided ?? '')}>
          {shown.map((t) => {
            const voters = votersOf(state, t.id, members);
            return (
              <button key={t.id} className="card z-glass" onClick={(e) => { zoen.haptics.tap(); setDetail({ id: t.id, from: (e.currentTarget.querySelector('.ph') as HTMLElement).getBoundingClientRect() }); }}
                aria-label={`${t.name}, ${t.park}, ${t.miles} miles, ${t.time}, ${t.difficulty}, ${voters.length} votes`}>
                <div className="ph"><Photo trail={t.id} /></div>
                <div className="meta">
                  <b>{t.name}</b>{state.decided === t.id && <span className="going"> · {L('Going')}</span>}
                  <div className="park">{t.park}</div>
                  <div className="line">
                    <span>{t.miles} mi • {t.time} • {L(t.difficulty)}{here ? ` • ${Math.round(haversineMi(here, t.trailhead))} ${L('mi away')}` : ''}</span>
                    {voters.length > 0 && <AvatarStack members={voters} />}
                  </div>
                </div>
              </button>
            );
          })}
        </Carousel>
      </div>

      {detail && <Detail id={detail.id} from={detail.from} state={state} members={members} me={me} here={here} onLocate={locate} onVote={vote} call={call} hosted={hosted} say={say} onClose={() => setDetail(null)} />}
      {compare && <Compare state={state} members={members} me={me} onVote={vote} onClose={() => setCompare(false)} />}
      {toast && <div className="toast z-glass" role="status">{toast}</div>}
    </>
  );
}

// ── map ──

function MapView({ trail }: { trail: Trail }) {
  const [online, setOnline] = useState(false);
  useEffect(() => {
    let mounted = true;
    zoen.ready().then(() => {
      if (mounted) setOnline(zoen.capabilities.has('net:tiles.openfreemap.org') || new URLSearchParams(location.search).has('net'));
    });
    return () => { mounted = false; };
  }, []);
  return online ? <OnlineMapView trail={trail} /> : <OfflineMap trail={trail} />;
}

function OnlineMapView({ trail }: { trail: Trail }) {
  const el = useRef<HTMLDivElement>(null);
  const map = useRef<maplibregl.Map | null>(null);
  const marker = useRef<maplibregl.Marker | null>(null);
  const ready = useRef(false);
  const latestTrail = useRef(trail);
  const [failed, setFailed] = useState(false);

  const show = (t: Trail, animate: boolean) => {
    const m = map.current; if (!m || !ready.current) return;
    (m.getSource('route') as maplibregl.GeoJSONSource).setData({ type: 'Feature', properties: {}, geometry: { type: 'LineString', coordinates: t.line } });
    const mid = t.line[Math.floor(t.line.length / 2)];
    marker.current!.setLngLat(mid);
    (marker.current!.getElement().firstChild as HTMLElement).textContent = t.name;
    m.fitBounds(bounds(t.line), { padding: { top: 150, bottom: 300, left: 50, right: 50 }, duration: animate ? 1200 : 0, easing: springEase, maxZoom: 14.5 });
  };

  useEffect(() => {
    if (failed || !el.current) return;
    let m: maplibregl.Map;
    try {
      m = new maplibregl.Map({ container: el.current, style: ONLINE_STYLE, center: trail.trailhead, zoom: 11, attributionControl: { compact: true }, pitchWithRotate: false, dragRotate: false });
    } catch { setFailed(true); return; }
    map.current = m;
    m.on('error', () => { if (!ready.current) setFailed(true); });
    const pin = document.createElement('div');
    const label = document.createElement('div'); label.className = 'marker'; pin.appendChild(label);
    marker.current = new maplibregl.Marker({ element: pin, anchor: 'bottom', offset: [0, -6] }).setLngLat(trail.trailhead).addTo(m);
    m.on('load', () => {
      const current = latestTrail.current;
      m.addSource('route', { type: 'geojson', data: { type: 'Feature', properties: {}, geometry: { type: 'LineString', coordinates: current.line } } });
      m.addLayer({ id: 'route-casing', type: 'line', source: 'route', paint: { 'line-color': '#ffffff', 'line-width': 9, 'line-opacity': 0.95 }, layout: { 'line-cap': 'round', 'line-join': 'round' } });
      m.addLayer({ id: 'route', type: 'line', source: 'route', paint: { 'line-color': '#3D7A28', 'line-width': 4.5 }, layout: { 'line-cap': 'round', 'line-join': 'round' } });
      ready.current = true;
      show(current, false);
    });
    return () => { ready.current = false; map.current = null; marker.current = null; m.remove(); };
  }, [failed]);

  useEffect(() => { latestTrail.current = trail; show(trail, true); }, [trail]);

  return failed ? <OfflineMap trail={trail} /> : <div ref={el} className="map" data-map="online" aria-label={`Map of ${trail.name}`} role="img" />;
}

// ── detail (matched expansion from the card photo) ──

function Detail({ id, from, state, members, me, here, onLocate, onVote, call, hosted, say, onClose }: {
  id: TrailId; from: DOMRect; state: HikeState; members: Member[]; me: Member; here: [number, number] | null;
  onLocate: () => void; onVote: (id: TrailId) => void; call: (tool: string, args?: any) => Promise<any>; hosted: boolean; say: (t: string) => void; onClose: () => void;
}) {
  const t = byId(id);
  const hero = useRef<HTMLDivElement>(null);
  const body = useRef<HTMLDivElement>(null);
  const page = useRef<HTMLDivElement>(null);
  const [mine, setMine] = useState<PickedPhoto[]>([]);
  const voters = votersOf(state, id, members);
  const iVoted = voters.some((v) => v.name === me.name);
  const decided = state.decided === id;

  // FLIP: the hero starts exactly where the card photo was and springs to full width.
  const animate = (open: boolean, done?: () => void) => {
    const h = hero.current!, b = body.current!;
    const to = h.getBoundingClientRect();
    const sx = from.width / to.width, sy = from.height / to.height;
    const dx = from.left - to.left, dy = from.top - to.top;
    page.current!.style.background = 'transparent';
    spring((p) => {
      const k = open ? p : 1 - p;
      h.style.transform = `translate(${lerp(dx, 0, k)}px, ${lerp(dy, 0, k)}px) scale(${lerp(sx, 1, k)}, ${lerp(sy, 1, k)})`;
      h.style.borderRadius = `${lerp(18 / Math.min(sx, 1), 0, Math.min(1, k))}px`;
      b.style.opacity = String(Math.max(0, Math.min(1, (k - 0.35) / 0.65)));
      b.style.transform = `translateY(${lerp(40, 0, Math.min(1, k))}px)`;
      page.current!.style.backgroundColor = `color-mix(in srgb, var(--z-bg) ${Math.round(Math.min(1, k) * 100)}%, transparent)`;
    }, { duration: open ? 0.5 : 0.4, bounce: open ? 0.12 : 0, onDone: done });
  };

  useLayoutEffect(() => { hero.current!.style.transformOrigin = '0 0'; animate(true); }, []);
  const close = () => { zoen.haptics.tap(); page.current!.scrollTo({ top: 0 }); animate(false, onClose); };

  const addToCalendar = async () => {
    const start = state.dayMs ?? nextSaturday(8);
    try {
      const r = await zoen.native.calendar.add({ title: `${state.title}: ${t.name}`, startMs: start, endMs: start + 7 * 3600_000, location: `${t.name}, ${t.park}`, notes: (state.itinerary ?? []).map((l) => `${l.time} ${l.text}`).join('\n') });
      say(r.saved ? L('Added to your calendar') : L('Not added'));
    } catch (e: any) { say(zoen.capabilities.has('calendar.add') ? e.message : L('Calendar isn’t available in this app host')); }
  };

  const pickPhotos = async () => {
    try { const ps = await zoen.native.photos.pick({ max: 4, maxSide: 900 }); setMine((m) => [...m, ...ps].slice(0, 6)); }
    catch (e: any) { say(zoen.capabilities.has('photos.pick') ? e.message : L('Photos aren’t available in this app host')); }
  };
  const shareMine = async () => {
    try { await call('hike_add_photos', { photos: mine.map((p) => p.token) }); setMine([]); zoen.haptics.success(); say(L('Shared with the group')); }
    catch (e: any) { say(e.message); }
  };

  return (
    <div className="page" ref={page} role="dialog" aria-label={t.name}>
      <GlassCircle label={L('Back')} onClick={close} style={{ position: 'fixed', left: 14, top: 'calc(env(safe-area-inset-top) + 10px)', zIndex: 30, width: 44, height: 44 }}>
        <svg width="18" height="18" viewBox="0 0 18 18" aria-hidden="true"><path d="M11 3 5 9l6 6" stroke="currentColor" strokeWidth="2.2" fill="none" strokeLinecap="round" strokeLinejoin="round" /></svg>
      </GlassCircle>
      <div className="hero" ref={hero} style={{ overflow: 'hidden' }}>
        <Carousel inset={0} gap={0}>
          {[0, 1, 2].map((i) => <div key={i}><Photo trail={id} i={i} /></div>)}
        </Carousel>
      </div>
      <div className="body" ref={body} style={{ opacity: 0 }}>
        <div className="tags"><Tag brand>{L(t.difficulty)}</Tag><Tag>{L(t.route)}</Tag>{decided && <Tag brand>{L('Going')} · {state.day}</Tag>}</div>
        <div><h1 className="z-title">{t.name}</h1><p className="z-sub">{t.park}</p></div>
        <StatGrid>
          <Stat value={`${t.miles} mi`} label={L('Distance')} />
          <Stat value={`${t.climbFt.toLocaleString('en-US')} ft`} label={L('Elevation gain')} />
          <Stat value={t.time} label={L('Time')} />
        </StatGrid>
        <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
          <span style={{ color: 'var(--z-ink-2)', fontWeight: 600, fontSize: 14 }}>{here ? `${L('About')} ${Math.round(haversineMi(here, t.trailhead))} ${L('mi from you')}` : L('Distance from you')}</span>
          {!here && <Pill tone="glass" onClick={onLocate}>{L('Show')}</Pill>}
        </div>
        <section className="about"><h2 className="z-h">{L('About this trail')}</h2><p>{t.about}</p></section>
        <div className="tags">{t.attributes.map((a) => <Tag key={a}>{a}</Tag>)}</div>
        {voters.length > 0 && <div style={{ display: 'flex', alignItems: 'center', gap: 8, fontSize: 14, color: 'var(--z-ink-2)' }}><AvatarStack members={voters} size={26} /> {voters.map((v) => v.name).join(', ')}</div>}
        {decided && state.itinerary && (
          <section><h2 className="z-h">{L('Plan')}</h2><div className="plan">{state.itinerary.map((l, i) => <div key={i}><b>{l.time}</b><span>{l.text}</span></div>)}</div></section>
        )}
        {decided && (
          <section>
            <h2 className="z-h">{L('Album')}</h2>
            <div className="album">
              {state.album.map((p, i) => <img key={i} src={p.dataUrl} alt={`Photo by ${p.by}`} />)}
              {mine.map((p) => <img key={p.token} src={p.dataUrl} className="mine" alt="Your photo, not shared yet" />)}
              {state.album.length + mine.length === 0 && [0, 1, 2].map((i) => <div key={i} className="ph" style={{ opacity: 0.35, overflow: 'hidden', borderRadius: 14 }}><Photo trail={id} i={i} /></div>)}
            </div>
            <div style={{ display: 'flex', gap: 8, marginTop: 10 }}>
              <Pill tone="glass" onClick={pickPhotos}>{L('Add photos')}</Pill>
              {mine.length > 0 && <Pill tone="brand" onClick={shareMine}>{L('Share')} {mine.length} {L('with group')}</Pill>}
            </div>
          </section>
        )}
      </div>
      <div className="sticky z-glass">
        <Pill tone="glass" onClick={() => zoen.openLink(t.link)} label={L('Open trail page')}>{new URL(t.link).hostname.replace('www.', '')} ↗</Pill>
        {decided
          ? <Pill tone="brand" onClick={addToCalendar}>{L('Add to calendar')}</Pill>
          : <Pill tone="black" onClick={() => onVote(id)}>{iVoted ? `${L('Voted')} · ${voters.length}` : L('Vote')}</Pill>}
      </div>
    </div>
  );
}

function nextSaturday(hour: number) {
  const d = new Date(); d.setHours(hour, 0, 0, 0);
  d.setDate(d.getDate() + ((6 - d.getDay() + 7) % 7 || 7));
  return d.getTime();
}

// ── compare (pinned label column, interactive pop) ──

function Compare({ state, members, me, onVote, onClose }: { state: HikeState; members: Member[]; me: Member; onVote: (id: TrailId) => void; onClose: () => void }) {
  const page = useRef<HTMLDivElement>(null);
  const x = useRef(0);
  const set = (px: number) => { x.current = px; const w = innerWidth; page.current!.style.transform = `translateX(${px}px)`; page.current!.style.boxShadow = `-12px 0 30px rgba(0,0,0,${0.18 * (1 - px / w)})`; };
  const go = (to: number, done?: () => void) => { const from = x.current; spring((p) => set(lerp(from, to, p)), { duration: 0.42, bounce: to === 0 ? 0.12 : 0, onDone: done }); };
  useLayoutEffect(() => { set(innerWidth); go(0); }, []);
  const pop = () => { zoen.haptics.tap(); go(innerWidth, onClose); };

  // Edge swipe: drag from the left edge, release past a third (or flick) to pop.
  const drag = useRef<{ x0: number; t0: number; on: boolean } | null>(null);
  const onDown = (e: React.PointerEvent) => { if (e.clientX < 28) { drag.current = { x0: e.clientX, t0: performance.now(), on: true }; (e.target as Element).setPointerCapture?.(e.pointerId); } };
  const onMove = (e: React.PointerEvent) => { if (drag.current?.on) set(Math.max(0, e.clientX - drag.current.x0)); };
  const onUp = (e: React.PointerEvent) => {
    if (!drag.current?.on) return;
    const dx = e.clientX - drag.current.x0, v = dx / Math.max(1, performance.now() - drag.current.t0);
    drag.current = null;
    if (dx > innerWidth / 3 || v > 0.6) pop(); else go(0);
  };

  const rows: { label: string; get: (t: Trail) => React.ReactNode }[] = [
    { label: L('Distance'), get: (t) => `${t.miles} mi` },
    { label: L('Climb'), get: (t) => `${t.climbFt.toLocaleString('en-US')} ft` },
    { label: L('Time'), get: (t) => t.time },
    { label: L('Difficulty'), get: (t) => L(t.difficulty) },
    { label: L('Route'), get: (t) => L(t.route) },
    { label: L('Terrain'), get: (t) => t.terrain },
    { label: L('Shade'), get: (t) => t.shade },
    { label: L('Best for'), get: (t) => t.bestFor },
    { label: L('Your vote'), get: (t) => {
      const voters = votersOf(state, t.id, members); const mine = voters.some((v) => v.name === me.name);
      return state.decided
        ? (state.decided === t.id ? <span className="going">{L('Going')}</span> : <span style={{ color: 'var(--z-ink-3)' }}>—</span>)
        : <div style={{ display: 'grid', gap: 8, justifyItems: 'start' }}>{voters.length > 0 && <AvatarStack members={voters} />}<Pill tone={mine ? 'brand' : 'black'} onClick={() => onVote(t.id)}>{mine ? L('Voted') : L('Vote')}</Pill></div>;
    } },
  ];

  return (
    <div className="page" ref={page} role="dialog" aria-label="Compare hikes" onPointerDown={onDown} onPointerMove={onMove} onPointerUp={onUp} onPointerCancel={onUp} style={{ touchAction: 'pan-y' }}>
      <div className="cmp-head">
        <GlassCircle label={L('Back')} onClick={pop}><svg width="18" height="18" viewBox="0 0 18 18" aria-hidden="true"><path d="M11 3 5 9l6 6" stroke="currentColor" strokeWidth="2.2" fill="none" strokeLinecap="round" strokeLinejoin="round" /></svg></GlassCircle>
        <h1>{L('Compare hikes')}</h1>
      </div>
      <div className="cmp">
        <table>
          <thead><tr><th className="lab" aria-label="Trail" />{TRAILS.map((t) => (
            <th key={t.id} scope="col"><div className="ph"><Photo trail={t.id} /></div><b>{t.name}</b><div className="tags" style={{ marginTop: 6 }}><Tag brand>{L(t.difficulty)}</Tag></div></th>
          ))}</tr></thead>
          <tbody>{rows.map((r) => (
            <tr key={r.label}><th scope="row" className="lab">{r.label}</th>{TRAILS.map((t) => <td key={t.id}>{r.get(t)}</td>)}</tr>
          ))}</tbody>
        </table>
      </div>
    </div>
  );
}

zoen.ready();
createRoot(document.getElementById('root')!).render(<App />);
