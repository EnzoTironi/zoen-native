import { Fragment, useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';
import { voiceHold } from './voice-hold.ts';
import { message as reason } from './boundary.ts';
import type { Conversation, FileDoc, Message, Messages, VoiceMeta } from './messages.ts';
import { FileView, kindOf, thumbnail, useRecorder } from './media.tsx';
import { NativeAdaptiveShell } from './native-adaptive.tsx';
import { displayShort, displayTime } from './display-clock.ts';
import { NativeAvatar, type NativeAvatarIdentity } from './native-art.tsx';
import { NativeBubbleText } from './native-bubble-text.tsx';
import { NativeHikeChatCard, type NativeHikeApp } from './native-hike-card.tsx';
import type { NativeRootItem } from './native-root-screens.tsx';
import { NativeNavigation, type NativeNavigationTab } from './native-navigation.tsx';
import { NativeSpacesDirectory, NativeNotifications, NativeProfile } from './native-root-screens.tsx';
import { NativeNewMessage } from './native-new-message.tsx';
import type { NativeRootScreenData } from './native-root-data.ts';
import { NativeHomeStrip, type WidgetSnapshot } from './native-widget.tsx';
import { typingPeers, type TypingState } from './typing-state.ts';

export function homeSnapshots(conversations: readonly Conversation[]): WidgetSnapshot[] {
  const snapshots = new Map<string, WidgetSnapshot>();
  const entries = conversations.flatMap(c => c.messages).filter(m => !m.deleted && m.card?.snapshot).sort((a, b) => a.at_ms - b.at_ms);
  for (const entry of entries) {
    const snapshot = entry.card!.snapshot!;
    snapshots.set(snapshot.id, snapshot);
  }
  return [...snapshots.values()];
}
import { MessageEditor } from './message-editor.tsx';
import { MessageSearchResults } from './message-search-view.tsx';
import type { MessageHit } from './message-search.ts';
import { loadChatPreferences, saveChatPreferences, type ChatPreferences as Prefs } from './chat-preferences.ts';

/** How I keep a chat. Local to this account/browser until synced preferences reach the web core. */
function loadPrefs(identity: string): Record<string, Prefs> {
  try { return loadChatPreferences(localStorage, identity); } catch { return {}; }
}
const FOLLOW = 'zoen.thread-follow';
function loadFollow(): Record<string, boolean> { try { return JSON.parse(localStorage.getItem(FOLLOW) ?? '{}') as Record<string, boolean>; } catch { return {}; } }

const QUICK = ['👍', '❤️', '😂', '😮', '😢', '🙏'];
const time = displayTime;
const day = displayShort;
const english = () => document.documentElement.lang.toLowerCase().startsWith('en');
function Avatar({ name, size = 44, counterpart, kind = 'person', id }: { name: string; size?: number; counterpart?: Conversation['counterpart']; kind?: 'person' | 'group' | 'agent'; id?: string }) {
  return <NativeAvatar identity={{ kind, name, ...(id ? { id } : {}), ...(counterpart ?? {}) }} size={size} />;
}

function Icon({ name }: { name: 'plus' | 'send' | 'mic' | 'reply' | 'thread' | 'smile' | 'more' | 'close' | 'pin' | 'mute' | 'archive' | 'back' | 'search' | 'phone' | 'video' }) {
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
    phone: <path d="m7 3 3 5-2 2c1 3 3 5 6 6l2-2 5 3c0 3-2 5-5 4C9 19 5 15 3 8 2 5 4 3 7 3Z"/>,
    video: <><rect x="2" y="5" width="14" height="14" rx="3"/><path d="m16 10 6-4v12l-6-4"/></>,
  };
  return <svg className="zm-icon" viewBox="0 0 24 24" aria-hidden="true">{p[name]}</svg>;
}

function Ticks({ state }: { state: Message['ticks'] | 'sending' | 'failed' }) {
  const label = { sending: 'Enviando', failed: 'Não enviada', sent: 'Enviada', delivered: 'Entregue', read: 'Lida' }[state ?? 'sent'];
  if (state === 'sending') return <svg className="zm-ticks" viewBox="0 0 16 11" role="img" aria-label={label}><circle cx="8" cy="5.5" r="4" fill="none" stroke="currentColor" strokeWidth="1.4" /><path d="M8 3.5v2l1.4 1" fill="none" stroke="currentColor" strokeWidth="1.4" /></svg>;
  if (state === 'failed') return <span className="zm-failed" role="img" aria-label={label}>!</span>;
  return <svg className={`zm-ticks zm-ticks-${state}`} viewBox="0 0 18 11" role="img" aria-label={label}>
    <path d="M1 6l3 3 6-7" />{state !== 'sent' ? <path d="M7 9l1 0 6-7" /> : null}
  </svg>;
}

export function Bubble({ entry, own, first, onReact, onReply, onThread, onEdit, onDelete, onForward, inThread, messages, formatTime = time, direct = false, counterpart, showFace = true, authorPersona, item, people = [] }: {
  entry: Message; messages: Messages | null; own: boolean; first: boolean; inThread?: boolean; formatTime?: (ms: number) => string; direct?: boolean; counterpart?: Conversation['counterpart']; showFace?: boolean; authorPersona?: NativeAvatarIdentity | undefined; item?: NativeRootItem | undefined; people?: NativeAvatarIdentity[];
  onReact: (emoji: string | null) => void; onReply: () => void; onThread: () => void; onEdit: () => void; onDelete: () => void; onForward: () => void;
}) {
  const [picker, setPicker] = useState(false);
  const [emojiDraft, setEmojiDraft] = useState('');
  const [details, setDetails] = useState(false);
  const [menu, setMenu] = useState(false);
  const ordered = entry.entry !== null;
  const mine = entry.reactions.find((r) => r.mine)?.emoji ?? null;
  const state = entry.delivery === 'confirmed' ? entry.ticks : entry.delivery as 'sending' | 'failed';
  return <article className={`zm-row${own ? ' zm-own' : ''}${first ? ' zm-first' : ''}`} data-testid="message" id={`message-${entry.id}`} tabIndex={-1} onMouseLeave={() => { setPicker(false); setMenu(false); }}>
    {!own && first && !direct ? <span className="zm-author" style={authorPersona?.tintHex ? { color: authorPersona.tintHex } : undefined}>{entry.name}</span> : null}
    {!own && showFace ? <span className="zm-sender-face">{authorPersona ? <NativeAvatar identity={authorPersona} size={36}/> : <Avatar name={entry.name} counterpart={counterpart} size={36} />}</span> : null}
    <div className="zm-bubble-wrap">
      <div className={`zm-bubble${entry.deleted ? ' zm-deleted' : ''}${entry.file && !entry.deleted ? ` zm-media zm-media-${kindOf(entry.file.mime)}` : ''}`}>
        {entry.forwarded && !entry.deleted ? <span className="zm-forwarded">Encaminhada</span> : null}
        {entry.quote && !entry.deleted ? <span className="zm-quote"><b>{entry.quote.name || 'Mensagem'}</b>{entry.quote.text || 'Mensagem apagada'}</span> : null}
        {entry.file && !entry.deleted ? <FileView file={entry.file} messages={messages} /> : null}
        {!entry.file || entry.deleted ? <NativeBubbleText text={entry.deleted ? english() ? 'Message deleted' : 'Mensagem apagada' : entry.text} meta={<>{entry.edited && !entry.deleted ? english() ? 'edited · ' : 'editada · ' : ''}<time dateTime={new Date(entry.at_ms).toISOString()}>{formatTime(entry.at_ms)}</time>{own ? <Ticks state={state} /> : null}</>}/> : <>{entry.text ? <span className="zm-text">{entry.text}</span> : null}<span className="zm-meta"><time dateTime={new Date(entry.at_ms).toISOString()}>{formatTime(entry.at_ms)}</time>{own ? <Ticks state={state} /> : null}</span></>}

      </div>
      {ordered && !entry.deleted ? <div className="zm-actions" role="toolbar" aria-label="Ações da mensagem">
        <button aria-label="Reagir" aria-expanded={picker} onClick={() => { setEmojiDraft(''); setPicker((v) => !v); }}><Icon name="smile" /></button>
        {!inThread ? <><button aria-label="Responder" onClick={onReply}><Icon name="reply" /></button>
          <button aria-label="Responder em thread" onClick={onThread}><Icon name="thread" /></button></> : null}
        <button aria-label="Mais ações" aria-expanded={menu} onClick={() => setMenu((v) => !v)}><Icon name="more" /></button>
      </div> : null}
      {picker ? <div className="zm-scrim" onClick={() => setPicker(false)} onKeyDown={(e) => { if (e.key === 'Escape') setPicker(false); }}>
        <div className="zm-sheet" role="dialog" aria-modal="true" aria-label="Escolher reação" onClick={(e) => e.stopPropagation()}>
          <h2>Escolher reação</h2>
          <div>{QUICK.map((emoji) => <button key={emoji} type="button" className="zm-pill" aria-pressed={emoji === mine} onClick={() => { setPicker(false); onReact(emoji === mine ? null : emoji); }}>{emoji}</button>)}</div>
          <form onSubmit={(e) => { e.preventDefault(); const emoji = emojiDraft.trim(); if (emoji && new TextEncoder().encode(emoji).length <= 32) { setPicker(false); onReact(emoji === mine ? null : emoji); } }}>
            <p><label>Outro emoji <input aria-label="Outro emoji" placeholder="Digite ou cole um emoji" value={emojiDraft} maxLength={64} autoFocus onChange={(e) => setEmojiDraft(e.target.value)} /></label></p>
            <p>Use o teclado de emojis do seu aparelho para escolher qualquer emoji.</p>
            {new TextEncoder().encode(emojiDraft.trim()).length > 32 ? <p role="alert">Escolha um emoji de até 32 bytes.</p> : null}
            <button className="zm-pill" type="submit" disabled={!emojiDraft.trim() || new TextEncoder().encode(emojiDraft.trim()).length > 32}>Reagir</button>
          </form>
          {mine ? <button className="zm-pill" onClick={() => { setPicker(false); onReact(null); }}>Remover minha reação</button> : null}
          <button className="zm-pill" onClick={() => setPicker(false)}>Fechar</button>
        </div>
      </div> : null}
      {menu ? <div className="zm-menu" role="menu">
        <button role="menuitem" onClick={() => { setMenu(false); onForward(); }}>Encaminhar</button>
        {entry.file ? null : <button role="menuitem" onClick={() => { setMenu(false); void navigator.clipboard?.writeText(entry.text); }}>Copiar</button>}
        {own && !entry.file ? <><button role="menuitem" onClick={() => { setMenu(false); onEdit(); }}>Editar</button>
          </> : null}
        {own ? <button role="menuitem" className="zm-danger" onClick={() => { setMenu(false); onDelete(); }}>Apagar para todos</button> : null}
      </div> : null}
    </div>
    {entry.reactions.length ? <div className="zm-reactions">{entry.reactions.map((r) => <button key={r.emoji} className={r.mine ? 'zm-on' : ''} aria-label={`Ver quem reagiu com ${r.emoji}: ${r.count}`} onClick={() => setDetails(true)}>{r.emoji}{r.count > 1 ? <span>{r.count}</span> : null}</button>)}</div> : null}
    {details ? <div className="zm-scrim" onClick={() => setDetails(false)} onKeyDown={(e) => { if (e.key === 'Escape') setDetails(false); }}>
      <div className="zm-sheet" role="dialog" aria-modal="true" aria-label="Quem reagiu" onClick={(e) => e.stopPropagation()}>
        <h2>Quem reagiu</h2>
        {entry.reactions.map((r) => <section key={r.emoji} aria-label={`${r.emoji}: ${r.count} reações`}>
          <h3>{r.emoji} {r.count}</h3>
          {r.participants.length ? <ul>{r.participants.map((participant) => <li key={participant.identity}>{participant.name || 'Alguém'}</li>)}</ul> : <p>Os nomes de quem reagiu não estão disponíveis.</p>}
          <button className="zm-pill" onClick={() => { setDetails(false); onReact(r.mine ? null : r.emoji); }}>{r.mine ? 'Remover minha reação' : `Reagir com ${r.emoji}`}</button>
        </section>)}
        {!entry.reactions.length ? <p>Nenhuma reação.</p> : null}
        <button className="zm-pill" autoFocus onClick={() => setDetails(false)}>Fechar</button>
      </div>
    </div> : null}
    {entry.card && !entry.deleted ? <ItemChatCard card={entry.card} item={item} people={people} /> : null}
    {!inThread && entry.replies ? <button className="zm-thread-summary" onClick={onThread}>
      <span className="zm-stack">{entry.repliers.map((n) => <Avatar key={n} name={n} size={20} />)}</span>
      <b>{entry.replies === 1 ? '1 resposta' : `${entry.replies} respostas`}</b>{entry.last_reply_ms ? <span>{day(entry.last_reply_ms)}</span> : null}
    </button> : null}
  </article>;
}

function ItemChatCard({ card, item, people }: { card: NonNullable<Message['card']>; item?: NativeRootItem | undefined; people: NativeAvatarIdentity[] }) {
  const [notice, setNotice] = useState(false);
  const hike = useMemo<NativeHikeApp | null>(() => {
    if (item?.app?.appId !== 'hike') return null;
    try { const view: unknown = JSON.parse(item.app.viewJson); return view && typeof view === 'object' && !Array.isArray(view) ? { appId: item.app.appId, name: item.app.name, view: view as Record<string, unknown> } : null; }
    catch { return null; }
  }, [item]);
  const money = (cents: number) => new Intl.NumberFormat('en-US', { style: 'currency', currency: 'USD', minimumFractionDigits: cents % 100 === 0 ? 0 : 2 }).format(cents / 100);
  const over = card.totalCents != null && card.budgetCents != null && card.totalCents > card.budgetCents;
  return <>{hike ? <NativeHikeChatCard app={hike} people={people} onOpen={() => setNotice(true)}/> : <button className="zm-item-card" onClick={() => setNotice(true)}><span className="zm-item-title">{card.title}<span aria-hidden="true">›</span></span>
    {card.summary ? <span className="zm-item-summary">{card.summary}</span> : null}
    {card.totalCents != null ? <><span className="zm-item-budget">
      {card.budgetCents != null ? <span className="zm-item-track"><i style={{ width: `${Math.max(0, Math.min(100, card.totalCents / Math.max(1, card.budgetCents) * 100))}%`, background: over ? '#D64545' : undefined }} /></span> : null}
      <span><b>{money(card.totalCents)}</b>{card.budgetCents != null ? <> of {money(card.budgetCents)}<span>{money(Math.abs(card.budgetCents - card.totalCents))} {over ? 'over' : 'left'}</span></> : null}</span>
    </span><span className="zm-item-counts">☷ {card.lineCount} items{card.doneCount > 0 ? <span>{card.doneCount} done</span> : null}</span></> : null}
  </button>}{notice ? <div className="zm-scrim" onClick={() => setNotice(false)}><section className="zm-sheet" role="dialog" aria-modal="true" aria-label={card.title} onClick={e => e.stopPropagation()}><h2>{card.title}</h2><p>Open this item in the native Zoen app. Item details are not available on web yet.</p><button className="zm-pill" autoFocus onClick={() => setNotice(false)}>Close</button></section></div> : null}</>;
}

type Outgoing = { name: string; mime: string; bytes: Uint8Array; thumb?: Uint8Array | null; voice?: VoiceMeta };
export function Composer({ placeholder, disabled, reply, cancelReply, submit, autoFocus, attach, initialText }: {
  placeholder: string; disabled: boolean; reply?: Message | null; cancelReply?: () => void; autoFocus?: boolean; initialText?: string;
  submit: (text: string) => Promise<void>; attach?: (file: Outgoing) => Promise<void>;
}) {
  const [sending, setSending] = useState('');
  const picker = useRef<HTMLInputElement>(null);
  const share = (out: Outgoing, label: string) => { if (!attach) return Promise.resolve(); setSending(label); setError('');
    return attach(out).catch((e: unknown) => setError(reason(e))).finally(() => setSending('')); };
  const files = async (list: FileList | File[]) => {
    for (const f of Array.from(list)) {
      const bytes = new Uint8Array(await f.arrayBuffer());
      await share({ name: f.name || 'arquivo', mime: f.type || 'application/octet-stream', bytes, thumb: await thumbnail(f) },
        kindOf(f.type) === 'image' ? 'Enviando foto…' : kindOf(f.type) === 'video' ? 'Enviando vídeo…' : 'Enviando arquivo…');
    }
  };
  const recorder = useRecorder((clip) => { if (clip.seconds < 0.6) return; void share({ name: `Mensagem de voz.${clip.mime.includes('mp4') ? 'm4a' : 'webm'}`, mime: clip.mime, bytes: clip.bytes,
    voice: { ms: Math.round(clip.seconds * 1000), levels: clip.levels, transcript: '', reply: reply?.entry ? { to: reply.entry, thread: false } : null } }, 'Enviando mensagem de voz…'); },
    (why) => setError(why));
  const latestRecorder = useRef(recorder);
  latestRecorder.current = recorder;
  const gestureCleanup = useRef<(() => void) | null>(null);
  const suppressVoiceClick = useRef(false);
  const [voiceLocked, setVoiceLocked] = useState(true);
  useEffect(() => {
    const hidden = () => { if (document.hidden) { gestureCleanup.current?.(); latestRecorder.current.cancel(); } };
    document.addEventListener('visibilitychange', hidden);
    return () => { document.removeEventListener('visibilitychange', hidden); gestureCleanup.current?.(); latestRecorder.current.cancel(); };
  }, []);
  function beginVoice(e: React.PointerEvent<HTMLButtonElement>) {
    if (e.button !== 0 || gestureCleanup.current) return;
    e.preventDefault();
    suppressVoiceClick.current = true;
    setVoiceLocked(false);
    let started = false;
    const gesture = voiceHold(e.clientX, e.clientY, {
      send: () => latestRecorder.current.send(), cancel: () => latestRecorder.current.cancel(),
    });
    const timer = window.setTimeout(() => { started = true; void latestRecorder.current.start(); }, 230);
    const cleanup = () => {
      clearTimeout(timer);
      window.removeEventListener('pointermove', move);
      window.removeEventListener('pointerup', up);
      window.removeEventListener('pointercancel', cancel);
      document.removeEventListener('visibilitychange', hidden);
      gestureCleanup.current = null;
    };
    const move = (event: PointerEvent) => {
      if (event.pointerId !== e.pointerId || !started) return;
      const phase = gesture.move(event.clientX, event.clientY);
      if (phase === 'locked') { setVoiceLocked(true); cleanup(); }
      else if (phase === 'ended') cleanup();
    };
    const up = (event: PointerEvent) => {
      if (event.pointerId !== e.pointerId) return;
      cleanup();
      if (started) gesture.release();
      else { setVoiceLocked(true); void latestRecorder.current.start(); }
    };
    const cancel = () => { cleanup(); gesture.cancel(); };
    const hidden = () => { if (document.hidden) cancel(); };
    gestureCleanup.current = cancel;
    window.addEventListener('pointermove', move);
    window.addEventListener('pointerup', up);
    window.addEventListener('pointercancel', cancel);
    document.addEventListener('visibilitychange', hidden);
  }
  const [draft, setDraft] = useState(initialText ?? '');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const field = useRef<HTMLTextAreaElement>(null);
  useEffect(() => { if (reply) field.current?.focus(); }, [reply?.id]);
  useEffect(() => { const el = field.current; if (el) { el.style.height = 'auto'; el.style.height = `${Math.min(el.scrollHeight, 168)}px`; } }, [draft]);
  const has = draft.trim().length > 0;
  function go() {
    if (!has || busy || disabled) return;
    setBusy(true); setError('');
    void submit(draft).then(() => setDraft('')).catch((e: unknown) => setError(reason(e))).finally(() => { setBusy(false); field.current?.focus(); });
  }
  return <div className="zm-composer-zone" onDragOver={(e) => { if (attach && e.dataTransfer.types.includes('Files')) e.preventDefault(); }}
    onDrop={(e) => { if (!attach || !e.dataTransfer.files.length) return; e.preventDefault(); void files(e.dataTransfer.files); }}>
    {reply ? <div className="zm-replying"><span><b>{`Respondendo a ${reply.name}`}</b>{reply.text}</span>
      <button aria-label="Cancelar" onClick={() => { setDraft(''); cancelReply?.(); }}><Icon name="close" /></button></div> : null}
    {error ? <p className="zm-error" role="alert">{error}</p> : null}
    {sending ? <p className="zm-sending" role="status">{sending}</p> : null}
    {recorder.recording ? <div className="zm-composer zm-recording" role="group" aria-label="Gravando mensagem de voz">
      <div className="zm-recording-surface">
      <button type="button" className="zm-round" aria-label="Descartar gravação" onClick={recorder.cancel}><Icon name="close" /></button>
      <span className="zw-sr-only" role="status">{voiceLocked ? 'Recording locked' : 'Release to send. Slide left to cancel or up to lock.'}</span>
      <span className="zm-rec-dot" aria-hidden="true" /><span className="zm-voice-time">{recorder.clock(recorder.seconds)}</span>
      <span className="zm-wave zm-live" aria-hidden="true">{Array.from({ length: 28 }, (_, i) => recorder.recording!.levels[i - 28 + recorder.recording!.levels.length] ?? 0.08).map((p, i) => <i key={i} style={{ height: `${Math.round(p * 100)}%` }} />)}</span>
      </div>
      <button type="button" className="zm-send glass zm-has" aria-label="Enviar mensagem de voz" onClick={recorder.send}><span className="zm-swap"><Icon name="send" /></span></button>
    </div> : <form className="zm-composer" onSubmit={(e) => { e.preventDefault(); go(); }}>
      {attach ? <><button type="button" className="zm-attach" aria-label="Anexar foto, vídeo ou arquivo" disabled={disabled} onClick={() => picker.current?.click()}><Icon name="plus" /></button>
        <input ref={picker} type="file" multiple hidden onChange={(e) => { if (e.target.files) void files(e.target.files); e.target.value = ''; }} /></> : null}
      <label className="zm-field"><span className="zw-sr-only">Mensagem</span>
        <textarea ref={field} rows={1} value={draft} disabled={disabled} placeholder={placeholder} maxLength={32_768} autoFocus={autoFocus}
          onChange={(e) => setDraft(e.target.value)}
          onPaste={(e) => { if (attach && e.clipboardData.files.length) { e.preventDefault(); void files(e.clipboardData.files); } }}
          onKeyDown={(e) => { if (e.key === 'Enter' && !e.shiftKey && !e.nativeEvent.isComposing) { e.preventDefault(); go(); } if (e.key === 'Escape') cancelReply?.(); }} /></label>
      <button type={has ? 'submit' : 'button'} key={has ? 'send' : 'mic'} className={`zm-send glass${has ? ' zm-has' : ''}`} disabled={disabled || busy || (!has && !attach)}
        aria-label={has ? 'Enviar' : 'Gravar mensagem de voz'} style={!has ? { touchAction: 'none' } : undefined}
        onPointerDown={has ? undefined : beginVoice}
        onClick={has ? undefined : (e) => {
          if (e.detail > 0 && suppressVoiceClick.current) { suppressVoiceClick.current = false; return; }
          setVoiceLocked(true); void recorder.start();
        }}>
        <span className="zm-swap"><Icon name={has ? 'send' : 'mic'} /></span></button>
    </form>}
  </div>;
}

const fileLabel = (f: FileDoc) => ({ image: 'Foto', video: 'Vídeo', audio: 'Mensagem de voz', file: f.name } as const)[kindOf(f.mime)];

function ChatRow({ c, identity, active, prefs, open, set }: { c: Conversation; identity: string; active: boolean; prefs: Prefs; open: () => void; set: (p: Prefs) => void }) {
  const last = [...c.messages].reverse().find((m) => !m.thread);
  const [menu, setMenu] = useState(false);
  const preview = last ? (last.deleted ? 'Mensagem apagada' : `${c.kind !== 'direct' && last.author !== identity && last.name ? `${last.name}: ` : ''}${last.file ? fileLabel(last.file) : last.text}`) : c.waiting ? 'Preparando a conversa…' : 'Nenhuma mensagem ainda';
  return <li className={`zm-chat${active ? ' zm-active' : ''}${c.unread ? ' zm-unread' : ''}`} onContextMenu={(e) => { e.preventDefault(); setMenu(true); }} onMouseLeave={() => setMenu(false)}>
    <button className="zm-chat-open" onClick={open} aria-current={active ? 'true' : undefined} aria-description={c.mentions ? `${c.mentions} ${english() ? 'mentions' : 'menções'}` : undefined}>
      <Avatar name={c.title} id={c.id} counterpart={c.counterpart} kind={c.kind === 'direct' ? 'person' : 'group'} size={56} />
      <span className="zm-chat-body"><span className="zm-chat-top"><b>{c.title}</b>{last ? <time>{day(last.at_ms)}</time> : null}</span>
        <span className="zm-chat-bottom"><span className="zm-preview">{last?.author === identity && !last.deleted && last.ticks ? <Ticks state={last.ticks}/> : null}{preview}</span>
          {prefs.muted ? <span className="zm-flag" aria-label="Silenciada"><Icon name="mute" /></span> : null}
          {prefs.pinned ? <span className="zm-flag" aria-label="Fixada"><Icon name="pin" /></span> : null}
          {c.unread ? <span className={`zm-badge${prefs.muted ? ' zm-muted' : ''}`} aria-label={`${c.unread} não lidas`}>{c.unread > 99 ? '99+' : c.unread}</span> : null}</span></span>
    </button>
    <button className="zm-chat-more" aria-label={`Opções de ${c.title}`} onClick={() => setMenu((v) => !v)}><Icon name="more" /></button>
    {menu ? <div className="zm-menu zm-chat-menu" role="menu">
      <button role="menuitem" onClick={() => { setMenu(false); set({ ...prefs, pinned: !prefs.pinned }); }}>{prefs.pinned ? 'Desafixar' : 'Fixar'}</button>
      <button role="menuitem" onClick={() => { setMenu(false); set({ ...prefs, muted: !prefs.muted }); }}>{prefs.muted ? 'Reativar som' : 'Silenciar'}</button>
      <button role="menuitem" onClick={() => { setMenu(false); set({ ...prefs, archived: !prefs.archived }); }}>{prefs.archived ? 'Desarquivar' : 'Arquivar'}</button>
    </div> : null}
  </li>;
}

export type MessengerProps = {
  conversations: Conversation[]; identity: string; name: string; connected: boolean; status: string; typing?: TypingState;
  messages: React.RefObject<Messages | null>; footer: React.ReactNode; selected: string; select: (id: string) => void; children?: React.ReactNode; overlay?: React.ReactNode; initialDraft?: string; onSettings?: () => void;
  rootData?: NativeRootScreenData; initialDestination?: NativeNavigationTab;
};

export function Messenger(props: MessengerProps) {
  // Account switches must also discard drafts, selections and in-memory preferences.
  return <AccountMessenger key={props.identity} {...props} />;
}

function AccountMessenger({ conversations, identity, name, connected, status, typing = {}, messages, footer, selected, select, children, overlay, initialDraft, onSettings, rootData, initialDestination = 'chats' }: MessengerProps) {
  const [wide, setWide] = useState(() => window.matchMedia?.('(min-width: 768px)').matches ?? false);
  const [destination, setDestination] = useState<NativeNavigationTab>(initialDestination === 'notifications' && !wide ? 'chats' : initialDestination);
  const [notificationsOpen, setNotificationsOpen] = useState(initialDestination === 'notifications' && !wide);
  const [newMessageOpen, setNewMessageOpen] = useState(false);
  const data: NativeRootScreenData = rootData ?? {
    me: { kind: 'person', id: identity, name }, contacts: null, requests: null, agents: null, mentions: null, items: null,
    stats: null, grants: null, account: null, connection: { state: connected ? 'online' : 'offline', synced: null, pending: null }, syncMode: 'live',
  };
  const notificationCount = data.requests?.filter(request => request.status === 'pending').length;
  const people: NativeAvatarIdentity[] = [...(data.me ? [data.me] : []), ...(data.contacts ?? []), ...(data.agents ?? []).map(agent => agent.persona)];
  const canCompose = connected || data.syncMode === 'demo';
  const searchOpen = false;
  const [homeNotice, setHomeNotice] = useState<WidgetSnapshot | null>(null);
  useEffect(() => {
    if (!window.matchMedia) return;
    const query = window.matchMedia('(min-width: 768px)');
    const update = () => setWide(query.matches);
    query.addEventListener('change', update);
    return () => query.removeEventListener('change', update);
  }, []);
  const navigate = (id: string) => {
    if (conversations.some(c => c.id === id)) { setDestination('chats'); select(id); return; }
    if (id === 'new-chat') { setNewMessageOpen(true); return; }
    if ((id === 'notifications' || id === 'activity') && !wide) { setNotificationsOpen(true); return; }
    const tab = id === 'you' ? 'profile' : id === 'activity' ? 'notifications' : id === 'store' ? 'marketplace' : id === 'files' ? 'spaces' : id;
    if (tab === 'chats' || tab === 'spaces' || tab === 'notifications' || tab === 'marketplace' || tab === 'profile') { setDestination(tab); select(''); }
  };
  const [prefs, setPrefs] = useState(() => loadPrefs(identity));
  const [follow, setFollow] = useState(loadFollow);
  const [search, setSearch] = useState('');
  const [showArchived, setShowArchived] = useState(false);
  const [inboxFilter, setInboxFilter] = useState('all');
  const [reply, setReply] = useState<Message | null>(null);
  const [editing, setEditing] = useState<Message | null>(null);
  const [threadRoot, setThreadRoot] = useState<string | null>(null);
  const [forward, setForward] = useState<Message | null>(null);
  const [toast, setToast] = useState('');
  const end = useRef<HTMLDivElement>(null);
  const unreadDivider = useRef<HTMLDivElement>(null);
  const atBottom = useRef(true);
  const scrollState = useRef<{ chat: string | undefined; captured: boolean; firstUnread: string | null; known: Set<string> }>({ chat: undefined, captured: false, firstUnread: null, known: new Set() });
  const [searchTarget, setSearchTarget] = useState<MessageHit | null>(null);
  const savePrefs = (id: string, p: Prefs) => {
    const next = { ...prefs, [id]: p }; setPrefs(next);
    try { saveChatPreferences(localStorage, identity, next); } catch { /* Storage may be disabled. */ }
  };
  const toggleFollow = (root: string, on: boolean) => { const next = { ...follow, [root]: on }; setFollow(next); localStorage.setItem(FOLLOW, JSON.stringify(next)); };
  const lastAt = (c: Conversation) => c.messages.at(-1)?.at_ms ?? 0;
  const matches = conversations.filter((c) => (wide || inboxFilter === 'all' || c.kind === inboxFilter) && c.title.toLocaleLowerCase('pt-BR').includes(search.toLocaleLowerCase('pt-BR')));
  const ordered = [...matches].sort((a, b) => Number(!!prefs[b.id]?.pinned) - Number(!!prefs[a.id]?.pinned) || lastAt(b) - lastAt(a));
  const archived = ordered.filter((c) => prefs[c.id]?.archived);
  const list = showArchived ? archived : ordered.filter((c) => !prefs[c.id]?.archived);
  const conversation = conversations.find((c) => c.id === selected);
  const typingPeer = conversation ? typingPeers(typing, conversation.id)[0] : undefined;
  const typingText = typingPeer ? (conversation?.kind === 'direct'
    ? english() ? 'is typing…' : 'está digitando…'
    : english() ? `${typingPeer.name ?? 'Someone'} is typing…` : `${typingPeer.name ?? 'Alguém'} está digitando…`) : null;
  const timeline = useMemo(() => conversation?.messages.filter((m) => !m.thread) ?? [], [conversation]);
  const root = threadRoot ? conversation?.messages.find((m) => m.entry === threadRoot) : undefined;
  const replies = root ? conversation!.messages.filter((m) => m.thread === threadRoot) : [];
  const threadsUnread = conversations.reduce((n, c) => n + c.messages.filter((m) => m.thread && follow[m.thread] && m.author !== identity).length, 0);
  const fail = (e: unknown) => { setToast(reason(e)); setTimeout(() => setToast(''), 3200); };

  // Capture before markRead can clear the count. Core unread includes thread replies,
  // but only normal incoming messages (not item cards) can anchor the main divider.
  if (scrollState.current.chat !== conversation?.id) {
    scrollState.current = { chat: conversation?.id, captured: false, firstUnread: null, known: new Set() };
    atBottom.current = true;
  }
  if (!scrollState.current.captured && conversation?.messages.length) {
    const incoming = conversation.messages.filter(m => m.author !== identity && !m.card);
    const unread = Math.max(0, conversation.unread);
    scrollState.current.firstUnread = unread ? incoming.slice(-unread).find(m => !m.thread)?.id ?? null : null;
    scrollState.current.captured = true;
  }
  const firstUnread = scrollState.current.firstUnread;
  useLayoutEffect(() => {
    const state = scrollState.current;
    if (!timeline.length) return;
    const initial = state.known.size === 0;
    const appendedMine = timeline.some(m => !state.known.has(m.id) && m.author === identity);
    if (initial && firstUnread) unreadDivider.current?.scrollIntoView({ block: 'start' });
    else if (initial || appendedMine || atBottom.current) end.current?.scrollIntoView({ block: 'end' });
    state.known = new Set(timeline.map(m => m.id));
  }, [conversation?.id, timeline, firstUnread, identity]);
  useEffect(() => { setReply(null); setEditing(null); setThreadRoot(null); }, [conversation?.id]);
  useEffect(() => {
    if (!conversation?.unread || !connected) return;
    // Read means seen: the tab is visible and the window has focus, like WhatsApp Web.
    const read = () => { if (document.visibilityState === 'visible' && document.hasFocus()) void messages.current?.markRead(conversation.id).catch(fail); };
    read(); document.addEventListener('visibilitychange', read); window.addEventListener('focus', read);
    return () => { document.removeEventListener('visibilitychange', read); window.removeEventListener('focus', read); };
  }, [conversation?.id, conversation?.unread, connected]);

  useEffect(() => {
    if (!searchTarget || conversation?.id !== searchTarget.space) return;
    if (searchTarget.message.thread && threadRoot !== searchTarget.message.thread) { setThreadRoot(searchTarget.message.thread); return; }
    const target = document.getElementById(`message-${searchTarget.message.id}`);
    if (target) { target.scrollIntoView({ block: 'center' }); target.focus(); setSearchTarget(null); }
  }, [searchTarget, conversation?.id, root?.id, threadRoot]);

  const space = conversation?.id ?? '';
  const act = {
    react: (m: Message) => (emoji: string | null) => { if (m.entry) void messages.current?.annotate(space, m.entry, emoji ? { React: { emoji } } : 'Unreact').catch(fail); },
    del: (m: Message) => () => {
      if (!m.entry || !window.confirm('Apagar esta mensagem para todos?')) return;
      const controller = messages.current;
      if (!controller) { fail(new Error('Recarregue o Zoen para tentar de novo.')); return; }
      void controller.annotate(space, m.entry, 'Delete').catch(fail);
    },
  };
  const bubbles = (items: Message[], inThread: boolean) => items.map((m, i) => <Fragment key={m.id}>
    {!inThread && m.id === firstUnread ? <div ref={unreadDivider} className="zm-unread-divider" data-testid="unread-divider"><span>{english() ? 'Unread messages' : 'Mensagens não lidas'}</span></div> : null}
    <Bubble entry={m} own={m.author === identity} inThread={inThread} direct={conversation?.kind === 'direct'} counterpart={conversation?.counterpart} messages={messages.current}
    formatTime={displayTime} authorPersona={people.find(person => person.id === m.author)} people={people} item={data.items?.find(item => item.id === m.card?.itemId)}
    first={i === 0 || !!items[i - 1]!.card || items[i - 1]!.author !== m.author || m.at_ms - items[i - 1]!.at_ms >= 300_000}
    showFace={conversation?.counterpart?.kind === 'agent' && conversation.counterpart.id === m.author || i === items.length - 1 || !!items[i + 1]!.card || items[i + 1]!.author !== m.author || items[i + 1]!.at_ms - m.at_ms >= 300_000}
    onReact={act.react(m)} onDelete={act.del(m)} onForward={() => setForward(m)}
    onReply={() => { setEditing(null); setReply(m); }} onEdit={() => { setReply(null); setEditing(m); }}
    onThread={() => { if (m.entry) { setThreadRoot(m.entry); if (follow[m.entry] === undefined) toggleFollow(m.entry, true); } }} /></Fragment>);

  const content = <div className={`zm-app zm-native-production${selected || children || destination !== 'chats' ? ' zm-detail-open' : ''}${root ? ' zm-with-thread' : ''}${conversation ? ' zm-conversation-open' : ''}`} data-root-tab={destination} data-selected-space={selected} data-sync-mode={data.syncMode ?? 'live'} data-connection={JSON.stringify(data.connection)} data-pending-requests={JSON.stringify(data.requests?.filter(request => request.status === 'pending').map(request => ({ agentId: request.agent.id, spaceId: request.spaceId, title: request.title, status: request.status })) ?? null)}>
    <aside className="zm-sidebar" aria-label="Conversas">
      {!searchOpen && !search ? <>
        <NativeHomeStrip snapshots={homeSnapshots(conversations)} notificationCount={data.requests?.filter(request => request.status === 'pending').length ?? null} onNotifications={() => navigate('notifications')} onOpen={setHomeNotice} />
        {homeNotice ? <div className="zm-scrim" onClick={() => setHomeNotice(null)}><section className="zm-sheet" role="dialog" aria-modal="true" aria-label={homeNotice.title} onClick={e => e.stopPropagation()}><h2>{homeNotice.title}</h2><p>Open this item in the native Zoen app. Item details are not available on web yet.</p><button className="zm-pill" autoFocus onClick={() => setHomeNotice(null)}>Close</button></section></div> : null}
      </> : null}
      {!showArchived && threadsUnread ? <p className="zm-threads-row"><Icon name="thread" />Threads que você segue<span className="zm-badge">{threadsUnread}</span></p> : null}
      {!showArchived && archived.length ? <button className="zm-archived-row" onClick={() => setShowArchived(true)}><Icon name="archive" />Arquivadas<span>{archived.length}</span></button> : null}
      {search.trim() ? <MessageSearchResults key={search.trim()} conversations={conversations} query={search} open={(hit) => {
        select(hit.space); setSearchTarget(hit);
      }} /> : null}
      <ul className="zm-chats">{list.map((c) => <ChatRow key={c.id} c={c} identity={identity} active={c.id === conversation?.id} prefs={prefs[c.id] ?? {}} open={() => { setDestination('chats'); select(c.id); }} set={(p) => savePrefs(c.id, p)} />)}</ul>
      {!list.length ? <p className="zm-empty">{search ? 'Nenhuma conversa encontrada.' : 'As conversas do seu celular aparecem aqui.'}</p> : null}
      {footer ? <footer className="zm-side-foot"><Avatar name={name} size={32} /><span><b>{name}</b><span className={connected ? 'zm-online' : 'zm-offline'} role="status">{status}</span></span>{footer}</footer> : null}
    </aside>
    <main className="zm-main">
      {children ?? (!conversation && destination === 'marketplace' ? <section className="native-root-screen" aria-label="Marketplace"><p>Marketplace is unavailable on this client.</p></section> : !conversation && destination === 'spaces' ? <NativeSpacesDirectory spaces={conversations} onOpen={select} /> : !conversation && destination === 'notifications' ? <NativeNotifications showsTitle={wide} requests={data.requests} agents={data.agents} mentions={data.mentions} items={data.items} onOpenSpace={select} /> : !conversation && destination === 'profile' ? <NativeProfile showsTitle={wide} me={data.me} spaces={conversations} agents={data.agents} stats={data.stats} grants={data.grants} account={data.account} connection={data.connection} syncMode={data.syncMode ?? 'live'} {...(onSettings ? { onSettings } : {})} onChats={() => navigate('chats')} {...(data.plannerLabel ? { plannerLabel: data.plannerLabel } : {})} /> : (conversation ? <>
        <header className="zm-chat-head"><button className="zm-round glass zm-mobile-back" aria-label="Back to Chats" onClick={() => select('')}><Icon name="back" /></button><div className="zm-title-slot"><button className="zm-title-pill" aria-label={conversation.title} onClick={() => fail(english() ? 'Chat details are not available on this client yet.' : 'Os detalhes da conversa ainda não estão disponíveis neste cliente.')}><Avatar name={conversation.title} id={conversation.id} counterpart={conversation.counterpart} kind={conversation.kind === 'direct' ? 'person' : 'group'} size={42} /><span className="zm-title-copy"><b>{conversation.title}</b>{typingText ? <span className="zm-live-typing" role="status" data-space={conversation.id} data-from={typingPeer?.identity}><svg viewBox="0 0 16 16" width="11" height="11" fill="none" stroke="currentColor" strokeWidth="1.2" aria-hidden="true"><path d="M2 2h9v7H6l-4 3Z"/><path d="M11 5h3v8l-3-2H8"/></svg>{typingText}</span> : conversation.kind !== 'direct' ? <span>{conversation.members} {english() ? 'people' : 'pessoas'}</span> : null}<span className="zm-title-chevron" aria-hidden="true">⌄</span></span></button></div><div className="zm-calls glass-pill">{(['phone', 'video'] as const).map(kind => <button key={kind} aria-label={kind === 'phone' ? 'Voice call' : 'Video call'} onClick={() => fail(english() ? 'Calls are not available on this client yet.' : 'Chamadas ainda não estão disponíveis neste cliente.')}><Icon name={kind}/></button>)}</div></header>
        <section className="zm-timeline" role="log" aria-label="Mensagens" aria-live="polite" onScroll={e => {
          const el = e.currentTarget;
          atBottom.current = el.scrollHeight - el.scrollTop - el.clientHeight <= 140;
        }}>
          <div className="zm-space-hero"><Avatar name={conversation.title} id={conversation.id} counterpart={conversation.counterpart} kind={conversation.kind === 'direct' ? 'person' : 'group'} size={conversation.kind === 'direct' ? 64 : 72} /></div>
          {conversation.waiting ? <p className="zm-note">Seu celular está preparando esta conversa para o navegador. Deixe ele conectado.</p> : null}
          {conversation.unreadable ? <p className="zm-note">{conversation.unreadable === 1 ? 'Uma mensagem anterior está' : `${conversation.unreadable} mensagens anteriores estão`} só no seu celular.</p> : null}
          {bubbles(timeline, false)}
          <div ref={end} />
        </section>
        <Composer key={conversation.id} initialText={initialDraft ?? ''} placeholder={english() ? 'Message' : 'Mensagem'} disabled={!canCompose || !conversation.can_send} reply={reply} autoFocus
          cancelReply={() => { setReply(null); setEditing(null); }}
          attach={async (file) => { const m = messages.current; if (!m) throw new Error('Recarregue o Zoen.'); await m.sendFile(space, file); if (file.voice?.reply) setReply(null); }}
          submit={async (text) => {
            const m = messages.current; if (!m) throw new Error('Recarregue o Zoen.');
            await m.post({ space, text, ...(reply?.entry ? { replyTo: reply.entry } : {}) }); setReply(null);
          }} />
      </> : <section className="zm-blank"><h2>Suas conversas</h2><p>{status}</p></section>))}
    </main>
    {!selected && !children ? <NativeNavigation selected={destination} offline={data.syncMode === 'live' && data.connection.state === 'offline'} onSelect={navigate} onNewMessage={() => setNewMessageOpen(true)} identity={data.me ?? { kind: 'person', id: identity, name }} {...(notificationCount === undefined ? {} : { notificationCount })} /> : null}
    {overlay}
    {notificationsOpen ? <div className="zm-notifications-overlay" role="presentation" onKeyDown={event => { if (event.key === 'Escape') setNotificationsOpen(false); }}><section className="zm-notifications-sheet" role="dialog" aria-modal="true" aria-label="Notifications"><header className="zm-notifications-head"><span className="zm-sheet-grabber" aria-hidden="true"/><button className="zm-round glass" aria-label="Close notifications" onClick={() => setNotificationsOpen(false)}><Icon name="close"/></button><h1>Notifications</h1></header><NativeNotifications requests={data.requests} agents={data.agents} mentions={data.mentions} items={data.items} onOpenSpace={id => { setNotificationsOpen(false); setDestination('chats'); select(id); }}/></section></div> : null}
    {newMessageOpen ? <NativeNewMessage items={data.items} conversations={conversations} onOpenConversation={id => { setNewMessageOpen(false); select(id); }} onOpenMessage={hit => { setNewMessageOpen(false); select(hit.space); setSearchTarget(hit); }} contacts={data.contacts} onClose={() => setNewMessageOpen(false)} onSelect={id => {
      const existing = conversations.find(conversation => conversation.kind === 'direct' && conversation.counterpart?.id === id);
      if (!existing) { fail('Starting a new direct chat is not available on this client yet.'); return; }
      setNewMessageOpen(false); select(existing.id);
    }} {...(data.connection.state === 'offline' ? { online: false } : data.connection.state === 'online' ? { online: true } : {})} /> : null}
    {root ? <aside className="zm-thread" aria-label="Thread">
      <header className="zm-thread-head"><b>Thread</b>
        <button className={`zm-follow${follow[root.entry!] ? ' zm-on' : ''}`} aria-pressed={!!follow[root.entry!]} onClick={() => toggleFollow(root.entry!, !follow[root.entry!])}>{follow[root.entry!] ? 'Seguindo' : 'Seguir'}</button>
        <button className="zm-round glass" aria-label="Fechar thread" onClick={() => setThreadRoot(null)}><Icon name="close" /></button></header>
      <section className="zm-timeline zm-thread-log" role="log" aria-label="Respostas">
        {bubbles([root], true)}
        <p className="zm-divider">{replies.length === 1 ? '1 resposta' : `${replies.length} respostas`}</p>
        {bubbles(replies, true)}
      </section>
      <Composer key={root.entry} placeholder="Responder na thread" disabled={!connected || !conversation?.can_send}
        submit={async (text) => { await messages.current?.post({ space, text, replyTo: root.entry!, thread: true }); if (follow[root.entry!] === undefined) toggleFollow(root.entry!, true); }} />
    </aside> : null}
    {editing ? <MessageEditor key={`${space}:${editing.id}`} entry={editing} close={() => setEditing(null)} save={async (text) => {
      const m = messages.current;
      if (!m || !editing.entry) throw new Error('Recarregue o Zoen para tentar de novo.');
      await m.annotate(space, editing.entry, { Edit: { text } });
    }} /> : null}
    {forward ? <div className="zm-scrim" onClick={() => setForward(null)}><div className="zm-sheet" role="dialog" aria-label="Encaminhar para" onClick={(e) => e.stopPropagation()}>
      <h2>Encaminhar para</h2>
      <ul>{conversations.filter((c) => c.can_send).map((c) => <li key={c.id}><button onClick={() => {
        const { text, file } = forward; setForward(null);
        void ((file ? messages.current?.forwardFile(c.id, file) : messages.current?.post({ space: c.id, text, forwarded: true }) ) ?? Promise.resolve()).then(() => select(c.id)).catch(fail);
      }}><Avatar name={c.title} id={c.id} kind={c.kind === 'direct' ? 'person' : 'group'} size={32} />{c.title}</button></li>)}</ul>
      <button className="zm-pill" onClick={() => setForward(null)}>Cancelar</button>
    </div></div> : null}
    {toast ? <p className="zm-toast" role="alert">{toast}</p> : null}
  </div>;
  return wide ? <NativeAdaptiveShell home={<NativeHomeStrip snapshots={homeSnapshots(conversations)} notificationCount={data.requests?.filter(request => request.status === 'pending').length ?? null} onNotifications={() => navigate('notifications')} onOpen={setHomeNotice}/>} selected={destination !== 'chats' ? destination : selected || null} onSelect={navigate} footer={footer} identity={data.me ?? { kind: 'person', id: identity, name }} {...(notificationCount === undefined ? {} : { notificationCount })} spaces={list.map(c => {
    const last = c.messages.at(-1);
    return { id: c.id, title: c.title, kind: c.kind, lastAtMs: last?.at_ms ?? 0, lastPreview: last ? last.deleted ? 'Mensagem apagada' : last.file ? fileLabel(last.file) : last.text : '', unread: c.unread, counterpart: c.counterpart ?? null };
  })}>{content}</NativeAdaptiveShell> : content;
}
