import { useRef, useState } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { flushSync } from 'react-dom';
import fixture from './fixtures/native-parity-v1.json';
import { Bubble, Composer } from './messenger.tsx';
import type { Message } from './messages.ts';
import { PARITY_FIXTURE, parityConfig, type ParityConfig } from './parity-config.ts';
import { NativeSettings, NativeOnboarding } from './native-secondary.tsx';
import { NativeNavigation } from './native-navigation.tsx';
import './native-parity.css';

declare const ZOEN_SOURCE_REVISION: string;
type Space = typeof fixture.spaces[number];
function Face({ space, size = 56 }: { space: Space; size?: number }) {
  const person = space.counterpart;
  return <span className="np-face" style={{ width: size, height: size, background: person?.tintHex ?? '#19B5A5' }}>{person?.initials ?? space.title.split(' ').slice(0, 2).map(w => w[0]).join('')}</span>;
}
function NativeChats({ open }: { open: (space: Space) => void }) {
  const [filter, setFilter] = useState('all');
  const [searchOpen, setSearchOpen] = useState(false);
  const [query, setQuery] = useState('');
  return <main className="np-inbox"><header><h1>Chats</h1><button aria-label="Notifications" className="glass np-bell"><svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round"><path d="M18 8a6 6 0 0 0-12 0c0 7-3 7-3 9h18c0-2-3-2-3-9M10 21h4" /></svg></button></header>
    <label className="np-filter"><svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round"><path d="M3 6h18M6 12h12M10 18h4" /></svg><select aria-label="Filter chats" value={filter} onChange={e => setFilter(e.target.value)}><option value="all">All chats</option><option value="direct">Direct chats</option><option value="group">Groups</option><option value="community">Communities</option></select></label>
    <div className="np-chat-list">{fixture.spaces.filter(s => (filter === 'all' || s.kind === filter) && s.title.toLowerCase().includes(query.toLowerCase())).map(space => <button className="np-chat-row" key={space.id} onClick={() => open(space)}><Face space={space} /><span className="np-chat-copy"><span className="np-chat-top"><b>{space.title}</b><time>{new Intl.DateTimeFormat('en-US', { hour: 'numeric', minute: '2-digit', timeZone: 'UTC' }).format(space.lastAtMs)}</time></span><span className="np-chat-bottom"><span>{space.lastPreview}</span>{space.unread > 0 && <b>{space.unread}</b>}</span></span></button>)}</div>
    <NativeNavigation selected="chats" search={{ open: searchOpen, value: query, onChange: setQuery, onOpenChange: setSearchOpen }} onSelect={() => {}} onCreate={() => {}} />
  </main>;
}
function toMessage(entry: Space['entries'][number]): Message {
  return { id: entry.id, entry: entry.id, text: entry.text, author: entry.author.id, name: entry.author.name, at_ms: entry.atMs, delivery: 'confirmed', failure: null, ticks: entry.delivery === 'read' ? 'read' : 'delivered', edited: entry.edited, deleted: entry.deleted, forwarded: false, reactions: [], quote: null, thread: null, replies: entry.threadReplies, repliers: [], last_reply_ms: null, file: null };
}
function NativeChat({ space, draft, back }: { space: Space; draft: boolean; back: () => void }) {
  const end = useRef<HTMLDivElement>(null);
  return <main className="np-chat"><header className="np-chat-toolbar"><button aria-label="Back" className="glass np-back" onClick={back}>‹</button><span className="glass np-chat-title"><Face space={space} size={28} /><b>{space.title}</b></span><button aria-label="More" className="glass np-back">···</button></header>
    <section className="np-timeline" role="log" aria-label="Messages">{space.entries.map((entry, i) => entry.card ? <article key={entry.id} className="np-card"><small>{entry.card.kindLabel}</small><h3>{entry.card.title}</h3><p>{entry.card.summary}</p></article> : <Bubble key={entry.id} entry={toMessage(entry)} own={entry.author.isMe} first={i === 0 || space.entries[i - 1]?.author.id !== entry.author.id} messages={null} formatTime={ms => new Intl.DateTimeFormat('en-US', { hour: 'numeric', minute: '2-digit', timeZone: 'UTC' }).format(ms)} onReact={() => {}} onReply={() => {}} onThread={() => {}} onEdit={() => {}} onDelete={() => {}} onForward={() => {}} />)}<div ref={end} /></section>
    <Composer initialText={draft ? 'See you tomorrow' : ''} placeholder="Message" disabled={false} submit={async () => { throw new Error('Read-only native visual fixture; no message was sent.'); }} />
  </main>;
}
function NativeParity({ config }: { config: ParityConfig }) {
  const [opened, setOpened] = useState<Space | null>(null);
  const marina = fixture.spaces.find(s => s.counterpart?.handle === 'marina');
  const coastal = fixture.spaces.find(s => s.kind === 'community');
  if (!marina || !coastal) throw new Error('Native parity seed is missing its required spaces.');
  const selected = opened ?? (config.screen === 'spaces' ? coastal : marina);
  return <div className={`native-parity np-${config.appearance}`} data-screen={config.screen} data-fixture-id={config.fixture_id}>
    {config.screen === 'settings' ? <NativeSettings me={fixture.me} /> : config.screen === 'onboarding' ? <NativeOnboarding /> : config.screen === 'chats' && !opened ? <NativeChats open={setOpened} /> : <NativeChat space={selected} draft={config.screen === 'composer'} back={() => setOpened(null)} />}
  </div>;
}

declare global { interface Window { __ZOEN_PARITY__?: { render: (value: unknown) => Promise<void>; manifest: Record<string, unknown> | null }; } }
/** Explicit opt-in fixture entry. It never opens a vault, signs in or replaces live message APIs. */
export function installNativeParity(element: HTMLElement) {
  let root: Root | undefined;
  const hook = { manifest: null as Record<string, unknown> | null, async render(value: unknown) {
    const config = parityConfig(value);
    if (config.source_revision !== ZOEN_SOURCE_REVISION) throw new Error('Parity source revision differs from this web build; rebuild before capture.');
    for (const [key, expected] of Object.entries(PARITY_FIXTURE)) if (fixture[key as keyof typeof fixture] !== expected) throw new Error(`Native fixture ${key} is stale.`);
    document.documentElement.lang = 'en-US';
    document.documentElement.style.colorScheme = config.appearance;
    root ??= createRoot(element);
    flushSync(() => root!.render(<NativeParity key={`${config.screen}-${config.appearance}`} config={config} />));
    await document.fonts.ready;
    await new Promise<void>(resolve => requestAnimationFrame(() => requestAnimationFrame(() => resolve())));
    await Promise.allSettled(document.getAnimations().filter(animation => animation.effect?.getTiming().iterations !== Infinity).map(animation => animation.finished));
    hook.manifest = { ...config, data_source: 'native-core-export', read_only: true };
  } };
  window.__ZOEN_PARITY__ = hook;
}
