export const time = (ms: number) => new Date(ms).toLocaleTimeString('pt-BR', { hour: '2-digit', minute: '2-digit' });
export function day(ms: number) {
  const d = new Date(ms); const today = new Date();
  if (d.toDateString() === today.toDateString()) return time(ms);
  const y = new Date(today); y.setDate(today.getDate() - 1);
  if (d.toDateString() === y.toDateString()) return 'Ontem';
  return d.toLocaleDateString('pt-BR', { day: '2-digit', month: '2-digit' });
}
const initials = (name: string) => name.replace(/^@/, '').trim().split(/\s+/).slice(0, 2).map((w) => w[0]?.toUpperCase() ?? '').join('') || '?';
function hue(seed: string) { let h = 0; for (const c of seed) h = (h * 31 + c.charCodeAt(0)) % 360; return h; }

export function Avatar({ name, size = 44 }: { name: string; size?: number }) {
  return <span className="zm-avatar" aria-hidden="true" style={{ width: size, height: size, fontSize: size * 0.38, background: `oklch(0.93 0.04 ${hue(name)})`, color: `oklch(0.38 0.08 ${hue(name)})` }}>{initials(name)}</span>;
}

export function Icon({ name }: { name: 'plus' | 'send' | 'mic' | 'reply' | 'thread' | 'smile' | 'more' | 'close' | 'pin' | 'mute' | 'archive' | 'back' | 'search' }) {
  const p: Record<string, React.ReactNode> = {
    send: <path d="M12 19V5m0 0-6 6m6-6 6 6" />,
    plus: <path d="M12 5v14M5 12h14" />,
    mic: <><rect x="9" y="3" width="6" height="11" rx="3" /><path d="M5 11a7 7 0 0 0 14 0M12 18v3" /></>,
    reply: <path d="M9 14 4 9l5-5M4 9h10a6 6 0 0 1 6 6v4" />,
    thread: <path d="M21 12a8 8 0 0 1-11.8 7L4 20l1.1-4.6A8 8 0 1 1 21 12Z" />,
    smile: <><circle cx="12" cy="12" r="9" /><path d="M8.5 14a4 4 0 0 0 7 0M9 9.5h.01M15 9.5h.01" /></>,
    more: <path d="M5 12h.01M12 12h.01M19 12h.01" />,
    close: <path d="M6 6l12 12M18 6 6 18" />,
    pin: <path d="M12 17v5M8 3h8l-1 6 3 3v2H6v-2l3-3-1-6Z" />,
    mute: <path d="M11 5 6 9H3v6h3l5 4V5ZM22 9l-6 6M16 9l6 6" />,
    archive: <path d="M3 4h18v4H3zM5 8v12h14V8M10 12h4" />,
    back: <path d="M15 18l-6-6 6-6" />,
    search: <><circle cx="11" cy="11" r="7" /><path d="m20 20-3.5-3.5" /></>,
  };
  return <svg className="zm-icon" viewBox="0 0 24 24" aria-hidden="true">{p[name]}</svg>;
}

