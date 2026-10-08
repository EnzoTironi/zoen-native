/**
 * @zoen/ui: React pieces for Zoen mini-apps. Tokens live in tokens.css (imported once by
 * the app); components here only compose them. Hooks wrap @zoen/miniapp-sdk.
 */
import { useEffect, useRef, useState, useSyncExternalStore, type CSSProperties, type ReactNode } from 'react';
import zoen, { type Member, type Theme } from '@zoen/miniapp-sdk';

// ── hooks ──

/** The mini-app's shared state, live (re-renders when anyone changes it). */
export function useZoenState<T = any>(): T | null {
  return useSyncExternalStore((cb) => zoen.state.subscribe(cb), () => zoen.state.get<T>());
}

export function useMembers(): Member[] {
  return useSyncExternalStore((cb) => zoen.members.subscribe(cb), () => zoen.members.list());
}

export function useTheme(): Theme {
  return useSyncExternalStore((cb) => zoen.theme.subscribe(cb), () => zoen.theme.get());
}

export function useReducedMotion(): boolean {
  const q = typeof matchMedia === 'function' ? matchMedia('(prefers-reduced-motion: reduce)') : null;
  const [r, setR] = useState(!!q?.matches);
  useEffect(() => { if (!q) return; const h = () => setR(q.matches); q.addEventListener('change', h); return () => q.removeEventListener('change', h); }, []);
  return r;
}

// ── spring (native-feeling motion without a library) ──

/** A critically-damped-ish spring like SwiftUI's `.spring(duration:bounce:)`. Calls
 *  `onFrame(progress)` from 0 to 1 (may overshoot slightly when bounce > 0). */
export function spring(onFrame: (p: number) => void, { duration = 0.45, bounce = 0.15, onDone }: { duration?: number; bounce?: number; onDone?: () => void } = {}) {
  const reduce = typeof matchMedia === 'function' && matchMedia('(prefers-reduced-motion: reduce)').matches;
  if (reduce) { onFrame(1); onDone?.(); return () => {}; }
  // SwiftUI mapping: stiffness = (2π/duration)², damping from bounce.
  const omega = (2 * Math.PI) / duration;
  const zeta = 1 - bounce;
  let x = 0, v = 0, raf = 0, last = performance.now(), stopped = false;
  const step = (now: number) => {
    if (stopped) return;
    const dt = Math.min(0.032, (now - last) / 1000); last = now;
    const a = -omega * omega * (x - 1) - 2 * zeta * omega * v;
    v += a * dt; x += v * dt;
    onFrame(x);
    if (Math.abs(1 - x) < 0.0008 && Math.abs(v) < 0.002) { onFrame(1); onDone?.(); return; }
    raf = requestAnimationFrame(step);
  };
  raf = requestAnimationFrame(step);
  return () => { stopped = true; cancelAnimationFrame(raf); };
}

export const lerp = (a: number, b: number, t: number) => a + (b - a) * t;

// ── components ──

export function GlassPill({ children, style, onClick, label }: { children: ReactNode; style?: CSSProperties; onClick?: () => void; label?: string }) {
  return <button className="z-pill z-glass" style={style} onClick={() => { zoen.haptics.tap(); onClick?.(); }} aria-label={label}>{children}</button>;
}

export function Pill({ children, tone = 'black', style, onClick, label, disabled }: { children: ReactNode; tone?: 'black' | 'brand' | 'glass'; style?: CSSProperties; onClick?: () => void; label?: string; disabled?: boolean }) {
  const cls = tone === 'glass' ? 'z-pill z-glass' : `z-pill ${tone}`;
  return <button className={cls} style={{ opacity: disabled ? 0.5 : 1, ...style }} disabled={disabled} aria-label={label} onClick={() => { zoen.haptics.tap(); onClick?.(); }}>{children}</button>;
}

export function Tag({ children, brand }: { children: ReactNode; brand?: boolean }) {
  return <span className={`z-pill tag${brand ? ' brand-soft' : ''}`}>{children}</span>;
}

export function GlassCircle({ children, onClick, label, style }: { children: ReactNode; onClick?: () => void; label: string; style?: CSSProperties }) {
  return <button className="z-circle z-glass" style={style} aria-label={label} onClick={() => { zoen.haptics.tap(); onClick?.(); }}>{children}</button>;
}

export function Stat({ value, label }: { value: ReactNode; label: string }) {
  return <div className="z-stat"><b>{value}</b><span>{label}</span></div>;
}

export function StatGrid({ children }: { children: ReactNode }) { return <div className="z-stat-grid">{children}</div>; }

export function Avatar({ m, size = 22 }: { m: Pick<Member, 'initials' | 'color' | 'name'>; size?: number }) {
  return <span className="z-avatar" title={m.name} style={{ background: m.color, width: size, height: size, fontSize: size * 0.45 }}>{m.initials}</span>;
}

export function AvatarStack({ members, max = 4, size = 22 }: { members: Pick<Member, 'initials' | 'color' | 'name'>[]; max?: number; size?: number }) {
  return <span className="z-avatars" aria-label={members.map((m) => m.name).join(', ')}>{members.slice(0, max).map((m, i) => <Avatar key={i} m={m} size={size} />)}</span>;
}

/** Tiny horizontal bars (the "small chart lib": no dependency). */
export function Bars({ data, color = 'var(--z-action)' }: { data: { label: string; value: number }[]; color?: string }) {
  const max = Math.max(1, ...data.map((d) => d.value));
  return (
    <div style={{ display: 'grid', gap: 6 }}>
      {data.map((d) => (
        <div key={d.label} style={{ display: 'grid', gridTemplateColumns: '84px 1fr 24px', alignItems: 'center', gap: 8, fontSize: 13 }}>
          <span style={{ color: 'var(--z-ink-2)', fontWeight: 600, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{d.label}</span>
          <span style={{ height: 8, borderRadius: 4, background: 'var(--z-muted)', overflow: 'hidden' }}>
            <span style={{ display: 'block', height: '100%', width: `${(d.value / max) * 100}%`, background: color, borderRadius: 4, transition: 'width .5s var(--z-spring)' }} />
          </span>
          <b style={{ fontVariantNumeric: 'tabular-nums', textAlign: 'right' }}>{d.value}</b>
        </div>
      ))}
    </div>
  );
}

/** Horizontal scroll-snap carousel that reports which card is centred. */
export function Carousel({ children, onIndex, gap = 12, inset = 16, style }: { children: ReactNode[]; onIndex?: (i: number) => void; gap?: number; inset?: number; style?: CSSProperties }) {
  const ref = useRef<HTMLDivElement>(null);
  const last = useRef(-1);
  useEffect(() => {
    const el = ref.current; if (!el) return;
    let t = 0;
    const h = () => {
      cancelAnimationFrame(t);
      t = requestAnimationFrame(() => {
        const mid = el.scrollLeft + el.clientWidth / 2;
        let best = 0, bd = Infinity;
        Array.from(el.children).forEach((c, i) => { const e = c as HTMLElement; const d = Math.abs(e.offsetLeft + e.offsetWidth / 2 - mid); if (d < bd) { bd = d; best = i; } });
        if (best !== last.current) { last.current = best; zoen.haptics.select(); onIndex?.(best); }
      });
    };
    el.addEventListener('scroll', h, { passive: true });
    return () => el.removeEventListener('scroll', h);
  }, [onIndex]);
  return <div ref={ref} className="z-snap-x" style={{ gap, padding: `0 ${inset}px`, scrollPaddingInline: inset, ...style }}>{children}</div>;
}

export { zoen };
