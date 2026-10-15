import { useEffect, useMemo, useRef, useState } from 'react';
import { message as reason } from './boundary.ts';
import type { Conversation, FileDoc, Message, Messages } from './messages.ts';
import { FileView, kindOf, thumbnail, useRecorder } from './media.tsx';

/** How I keep a chat. Local to this browser until the synced chat preferences reach the web core. */
type Prefs = { pinned?: boolean; muted?: boolean; archived?: boolean };
const PREFS = 'zoen.chat-prefs';
function loadPrefs(): Record<string, Prefs> { try { return JSON.parse(localStorage.getItem(PREFS) ?? '{}') as Record<string, Prefs>; } catch { return {}; } }
const FOLLOW = 'zoen.thread-follow';
function loadFollow(): Record<string, boolean> { try { return JSON.parse(localStorage.getItem(FOLLOW) ?? '{}') as Record<string, boolean>; } catch { return {}; } }

const QUICK = ['👍', '❤️', '😂', '😮', '😢', '🙏'];
const time = (ms: number) => new Date(ms).toLocaleTimeString('pt-BR', { hour: '2-digit', minute: '2-digit' });
function day(ms: number) {
  const d = new Date(ms); const today = new Date();
  if (d.toDateString() === today.toDateString()) return time(ms);
  const y = new Date(today); y.setDate(today.getDate() - 1);
  if (d.toDateString() === y.toDateString()) return 'Ontem';
  return d.toLocaleDateString('pt-BR', { day: '2-digit', month: '2-digit' });
}
const initials = (name: string) => name.replace(/^@/, '').trim().split(/\s+/).slice(0, 2).map((w) => w[0]?.toUpperCase() ?? '').join('') || '?';
function hue(seed: string) { let h = 0; for (const c of seed) h = (h * 31 + c.charCodeAt(0)) % 360; return h; }

function Avatar({ name, size = 44 }: { name: string; size?: number }) {
  return <span className="zm-avatar" aria-hidden="true" style={{ width: size, height: size, fontSize: size * 0.38, background: `oklch(0.93 0.04 ${hue(name)})`, color: `oklch(0.38 0.08 ${hue(name)})` }}>{initials(name)}</span>;
}

function Icon({ name }: { name: 'plus' | 'send' | 'mic' | 'reply' | 'thread' | 'smile' | 'more' | 'close' | 'pin' | 'mute' | 'archive' | 'back' | 'search' }) {
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

function Ticks({ state }: { state: Message['ticks'] | 'sending' | 'failed' }) {
  const label = { sending: 'Enviando', failed: 'Não enviada', sent: 'Enviada', delivered: 'Entregue', read: 'Lida' }[state ?? 'sent'];
  if (state === 'sending') return <svg className="zm-ticks" viewBox="0 0 16 11" role="img" aria-label={label}><circle cx="8" cy="5.5" r="4" fill="none" stroke="currentColor" strokeWidth="1.4" /><path d="M8 3.5v2l1.4 1" fill="none" stroke="currentColor" strokeWidth="1.4" /></svg>;
  if (state === 'failed') return <span className="zm-failed" role="img" aria-label={label}>!</span>;
  return <svg className={`zm-ticks zm-ticks-${state}`} viewBox="0 0 18 11" role="img" aria-label={label}>
    <path d="M1 6l3 3 6-7" />{state !== 'sent' ? <path d="M7 9l1 0 6-7" /> : null}
  </svg>;
}

function Bubble({ entry, own, first, onReact, onReply, onThread, onEdit, onDelete, onForward, inThread, messages }: {
  entry: Message; messages: Messages | null; own: boolean; first: boolean; inThread?: boolean;
  onReact: (emoji: string | null) => void; onReply: () => void; onThread: () => void; onEdit: () => void; onDelete: () => void; onForward: () => void;
}) {
  const [picker, setPicker] = useState(false);
  const [menu, setMenu] = useState(false);
  const ordered = entry.entry !== null;
  const mine = entry.reactions.find((r) => r.mine)?.emoji ?? null;
  const state = entry.delivery === 'confirmed' ? entry.ticks : entry.delivery as 'sending' | 'failed';
  return <article className={`zm-row${own ? ' zm-own' : ''}${first ? ' zm-first' : ''}`} data-testid="message" onMouseLeave={() => { setPicker(false); setMenu(false); }}>
    {!own && first ? <span className="zm-author">{entry.name}</span> : null}
    <div className="zm-bubble-wrap">
      <div className={`zm-bubble${entry.deleted ? ' zm-deleted' : ''}${entry.file && !entry.deleted ? ` zm-media zm-media-${kindOf(entry.file.mime)}` : ''}`}>
        {entry.forwarded && !entry.deleted ? <span className="zm-forwarded">Encaminhada</span> : null}
        {entry.quote && !entry.deleted ? <span className="zm-quote"><b>{entry.quote.name || 'Mensagem'}</b>{entry.quote.text || 'Mensagem apagada'}</span> : null}
        {entry.file && !entry.deleted ? <FileView file={entry.file} messages={messages} /> : <span className="zm-text">{entry.deleted ? 'Mensagem apagada' : entry.text}</span>}
        <span className="zm-meta">{entry.edited && !entry.deleted ? 'editada · ' : ''}<time dateTime={new Date(entry.at_ms).toISOString()}>{time(entry.at_ms)}</time>{own ? <Ticks state={state} /> : null}</span>
      </div>
      {ordered && !entry.deleted ? <div className="zm-actions" role="toolbar" aria-label="Ações da mensagem">
        <button aria-label="Reagir" onClick={() => setPicker((v) => !v)}><Icon name="smile" /></button>
        {!inThread ? <><button aria-label="Responder" onClick={onReply}><Icon name="reply" /></button>
          <button aria-label="Responder em thread" onClick={onThread}><Icon name="thread" /></button></> : null}
        <button aria-label="Mais ações" aria-expanded={menu} onClick={() => setMenu((v) => !v)}><Icon name="more" /></button>
      </div> : null}
      {picker ? <div className="zm-picker" role="menu">{QUICK.map((emoji) => <button key={emoji} role="menuitem" className={emoji === mine ? 'zm-on' : ''} onClick={() => { setPicker(false); onReact(emoji === mine ? null : emoji); }}>{emoji}</button>)}</div> : null}
      {menu ? <div className="zm-menu" role="menu">
        <button role="menuitem" onClick={() => { setMenu(false); onForward(); }}>Encaminhar</button>
        {entry.file ? null : <button role="menuitem" onClick={() => { setMenu(false); void navigator.clipboard?.writeText(entry.text); }}>Copiar</button>}
        {own && !entry.file ? <><button role="menuitem" onClick={() => { setMenu(false); onEdit(); }}>Editar</button>
          </> : null}
        {own ? <button role="menuitem" className="zm-danger" onClick={() => { setMenu(false); onDelete(); }}>Apagar para todos</button> : null}
      </div> : null}
    </div>
    {entry.reactions.length ? <div className="zm-reactions">{entry.reactions.map((r) => <button key={r.emoji} className={r.mine ? 'zm-on' : ''} aria-label={`${r.emoji} ${r.count}`} onClick={() => onReact(r.mine ? null : r.emoji)}>{r.emoji}{r.count > 1 ? <span>{r.count}</span> : null}</button>)}</div> : null}
    {!inThread && entry.replies ? <button className="zm-thread-summary" onClick={onThread}>
      <span className="zm-stack">{entry.repliers.map((n) => <Avatar key={n} name={n} size={20} />)}</span>
      <b>{entry.replies === 1 ? '1 resposta' : `${entry.replies} respostas`}</b>{entry.last_reply_ms ? <span>{day(entry.last_reply_ms)}</span> : null}
    </button> : null}
  </article>;
}

type Outgoing = { name: string; mime: string; bytes: Uint8Array; thumb?: Uint8Array | null };
function Composer({ placeholder, disabled, reply, cancelReply, editing, submit, autoFocus, attach }: {
  placeholder: string; disabled: boolean; reply?: Message | null; cancelReply?: () => void; editing?: Message | null; autoFocus?: boolean;
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
  const recorder = useRecorder((clip) => { if (clip.seconds < 0.6) return; void share({ name: `Mensagem de voz.${clip.mime.includes('mp4') ? 'm4a' : 'webm'}`, mime: clip.mime, bytes: clip.bytes }, 'Enviando mensagem de voz…'); },
    (why) => setError(why));
  const [draft, setDraft] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const field = useRef<HTMLTextAreaElement>(null);
  useEffect(() => { if (editing) { setDraft(editing.text); field.current?.focus(); } }, [editing?.id]);
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
    {reply || editing ? <div className="zm-replying"><span><b>{editing ? 'Editando mensagem' : `Respondendo a ${reply?.name}`}</b>{(editing ?? reply)?.text}</span>
      <button aria-label="Cancelar" onClick={() => { setDraft(''); cancelReply?.(); }}><Icon name="close" /></button></div> : null}
    {error ? <p className="zm-error" role="alert">{error}</p> : null}
    {sending ? <p className="zm-sending" role="status">{sending}</p> : null}
    {recorder.recording ? <div className="zm-composer zm-recording" role="group" aria-label="Gravando mensagem de voz">
      <button type="button" className="zm-round" aria-label="Descartar gravação" onClick={recorder.cancel}><Icon name="close" /></button>
      <span className="zm-rec-dot" aria-hidden="true" /><span className="zm-voice-time">{recorder.clock(recorder.seconds)}</span>
      <span className="zm-wave zm-live" aria-hidden="true">{Array.from({ length: 28 }, (_, i) => recorder.recording!.levels[i - 28 + recorder.recording!.levels.length] ?? 0.08).map((p, i) => <i key={i} style={{ height: `${Math.round(p * 100)}%` }} />)}</span>
      <button type="button" className="zm-send glass zm-has" aria-label="Enviar mensagem de voz" onClick={recorder.send}><span className="zm-swap"><Icon name="send" /></span></button>
    </div> : <form className="zm-composer" onSubmit={(e) => { e.preventDefault(); go(); }}>
      {attach ? <><button type="button" className="zm-attach" aria-label="Anexar foto, vídeo ou arquivo" disabled={disabled} onClick={() => picker.current?.click()}><Icon name="plus" /></button>
        <input ref={picker} type="file" multiple hidden onChange={(e) => { if (e.target.files) void files(e.target.files); e.target.value = ''; }} /></> : null}
      <label className="zm-field"><span className="zw-sr-only">Mensagem</span>
        <textarea ref={field} rows={1} value={draft} disabled={disabled} placeholder={placeholder} maxLength={32_768} autoFocus={autoFocus}
          onChange={(e) => setDraft(e.target.value)}
          onPaste={(e) => { if (attach && e.clipboardData.files.length) { e.preventDefault(); void files(e.clipboardData.files); } }}
          onKeyDown={(e) => { if (e.key === 'Enter' && !e.shiftKey && !e.nativeEvent.isComposing) { e.preventDefault(); go(); } if (e.key === 'Escape') cancelReply?.(); }} /></label>
      <button type={has ? 'submit' : 'button'} key={has ? 'send' : 'mic'} className={`zm-send glass${has ? ' zm-has' : ''}`} disabled={disabled || busy || (!has && (!attach || !!editing))}
        aria-label={has ? 'Enviar' : 'Gravar mensagem de voz'} onClick={has ? undefined : () => void recorder.start()}>
        <span className="zm-swap"><Icon name={has ? 'send' : 'mic'} /></span></button>
    </form>}
  </div>;
}

const fileLabel = (f: FileDoc) => ({ image: 'Foto', video: 'Vídeo', audio: 'Mensagem de voz', file: f.name } as const)[kindOf(f.mime)];

function ChatRow({ c, active, prefs, open, set }: { c: Conversation; active: boolean; prefs: Prefs; open: () => void; set: (p: Prefs) => void }) {
  const last = [...c.messages].reverse().find((m) => !m.thread);
  const [menu, setMenu] = useState(false);
  const preview = last ? (last.deleted ? 'Mensagem apagada' : `${c.kind !== 'direct' && last.name ? `${last.name}: ` : ''}${last.file ? fileLabel(last.file) : last.text}`) : c.waiting ? 'Preparando a conversa…' : 'Nenhuma mensagem ainda';
  return <li className={`zm-chat${active ? ' zm-active' : ''}${c.unread ? ' zm-unread' : ''}`} onContextMenu={(e) => { e.preventDefault(); setMenu(true); }} onMouseLeave={() => setMenu(false)}>
    <button className="zm-chat-open" onClick={open} aria-current={active ? 'true' : undefined}>
      <Avatar name={c.title} />
      <span className="zm-chat-body"><span className="zm-chat-top"><b>{c.title}</b>{last ? <time>{day(last.at_ms)}</time> : null}</span>
        <span className="zm-chat-bottom"><span className="zm-preview">{preview}</span>
          {prefs.muted ? <span className="zm-flag" aria-label="Silenciada"><Icon name="mute" /></span> : null}
          {prefs.pinned ? <span className="zm-flag" aria-label="Fixada"><Icon name="pin" /></span> : null}
          {c.mentions ? <span className="zm-badge zm-mention" aria-label={`${c.mentions} menções`}>@{c.mentions}</span> : c.unread ? <span className={`zm-badge${prefs.muted ? ' zm-muted' : ''}`} aria-label={`${c.unread} não lidas`}>{c.unread}</span> : null}</span></span>
    </button>
    <button className="zm-chat-more" aria-label={`Opções de ${c.title}`} onClick={() => setMenu((v) => !v)}><Icon name="more" /></button>
    {menu ? <div className="zm-menu zm-chat-menu" role="menu">
      <button role="menuitem" onClick={() => { setMenu(false); set({ ...prefs, pinned: !prefs.pinned }); }}>{prefs.pinned ? 'Desafixar' : 'Fixar'}</button>
      <button role="menuitem" onClick={() => { setMenu(false); set({ ...prefs, muted: !prefs.muted }); }}>{prefs.muted ? 'Reativar som' : 'Silenciar'}</button>
      <button role="menuitem" onClick={() => { setMenu(false); set({ ...prefs, archived: !prefs.archived }); }}>{prefs.archived ? 'Desarquivar' : 'Arquivar'}</button>
    </div> : null}
  </li>;
}

export function Messenger({ conversations, identity, name, connected, status, messages, footer, selected, select, children }: {
  conversations: Conversation[]; identity: string; name: string; connected: boolean; status: string;
  messages: React.RefObject<Messages | null>; footer: React.ReactNode; selected: string; select: (id: string) => void; children?: React.ReactNode;
}) {
  const [prefs, setPrefs] = useState(loadPrefs);
  const [follow, setFollow] = useState(loadFollow);
  const [search, setSearch] = useState('');
  const [showArchived, setShowArchived] = useState(false);
  const [reply, setReply] = useState<Message | null>(null);
  const [editing, setEditing] = useState<Message | null>(null);
  const [threadRoot, setThreadRoot] = useState<string | null>(null);
  const [forward, setForward] = useState<Message | null>(null);
  const [toast, setToast] = useState('');
  const end = useRef<HTMLDivElement>(null);
  const savePrefs = (id: string, p: Prefs) => { const next = { ...prefs, [id]: p }; setPrefs(next); localStorage.setItem(PREFS, JSON.stringify(next)); };
  const toggleFollow = (root: string, on: boolean) => { const next = { ...follow, [root]: on }; setFollow(next); localStorage.setItem(FOLLOW, JSON.stringify(next)); };
  const lastAt = (c: Conversation) => c.messages.at(-1)?.at_ms ?? 0;
  const matches = conversations.filter((c) => c.title.toLocaleLowerCase('pt-BR').includes(search.toLocaleLowerCase('pt-BR')));
  const ordered = [...matches].sort((a, b) => Number(!!prefs[b.id]?.pinned) - Number(!!prefs[a.id]?.pinned) || lastAt(b) - lastAt(a));
  const archived = ordered.filter((c) => prefs[c.id]?.archived);
  const list = showArchived ? archived : ordered.filter((c) => !prefs[c.id]?.archived);
  const conversation = conversations.find((c) => c.id === selected) ?? (children ? undefined : list[0]);
  const timeline = useMemo(() => conversation?.messages.filter((m) => !m.thread) ?? [], [conversation]);
  const root = threadRoot ? conversation?.messages.find((m) => m.entry === threadRoot) : undefined;
  const replies = root ? conversation!.messages.filter((m) => m.thread === threadRoot) : [];
  const threadsUnread = conversations.reduce((n, c) => n + c.messages.filter((m) => m.thread && follow[m.thread] && m.author !== identity).length, 0);
  const fail = (e: unknown) => { setToast(reason(e)); setTimeout(() => setToast(''), 3200); };

  useEffect(() => { end.current?.scrollIntoView({ block: 'end' }); }, [conversation?.id, timeline.length]);
  useEffect(() => { setReply(null); setEditing(null); setThreadRoot(null); }, [conversation?.id]);
  useEffect(() => {
    if (!conversation?.unread || !connected) return;
    // Read means seen: the tab is visible and the window has focus, like WhatsApp Web.
    const read = () => { if (document.visibilityState === 'visible' && document.hasFocus()) void messages.current?.markRead(conversation.id); };
    read(); document.addEventListener('visibilitychange', read); window.addEventListener('focus', read);
    return () => { document.removeEventListener('visibilitychange', read); window.removeEventListener('focus', read); };
  }, [conversation?.id, conversation?.unread, connected]);

  const space = conversation?.id ?? '';
  const act = {
    react: (m: Message) => (emoji: string | null) => { if (m.entry) void messages.current?.annotate(space, m.entry, emoji ? { React: { emoji } } : 'Unreact').catch(fail); },
    del: (m: Message) => () => { if (m.entry && window.confirm('Apagar esta mensagem para todos?')) void messages.current?.annotate(space, m.entry, 'Delete').catch(fail); },
  };
  const bubbles = (items: Message[], inThread: boolean) => items.map((m, i) => <Bubble key={m.id} entry={m} own={m.author === identity} inThread={inThread} messages={messages.current}
    first={i === 0 || items[i - 1]!.author !== m.author}
    onReact={act.react(m)} onDelete={act.del(m)} onForward={() => setForward(m)}
    onReply={() => { setEditing(null); setReply(m); }} onEdit={() => { setReply(null); setEditing(m); }}
    onThread={() => { if (m.entry) { setThreadRoot(m.entry); if (follow[m.entry] === undefined) toggleFollow(m.entry, true); } }} />);

  return <div className={`zm-app${root ? ' zm-with-thread' : ''}`}>
    <aside className="zm-sidebar" aria-label="Conversas">
      <header className="zm-side-head">
        {showArchived ? <button className="zm-round glass" aria-label="Voltar" onClick={() => setShowArchived(false)}><Icon name="back" /></button> : null}
        <h1>{showArchived ? 'Arquivadas' : 'Conversas'}</h1>
      </header>
      <label className="zm-search"><Icon name="search" /><span className="zw-sr-only">Buscar conversas</span>
        <input type="search" placeholder="Buscar" value={search} onChange={(e) => setSearch(e.target.value)} /></label>
      {!showArchived && threadsUnread ? <p className="zm-threads-row"><Icon name="thread" />Threads que você segue<span className="zm-badge">{threadsUnread}</span></p> : null}
      {!showArchived && archived.length ? <button className="zm-archived-row" onClick={() => setShowArchived(true)}><Icon name="archive" />Arquivadas<span>{archived.length}</span></button> : null}
      <ul className="zm-chats">{list.map((c) => <ChatRow key={c.id} c={c} active={c.id === conversation?.id} prefs={prefs[c.id] ?? {}} open={() => select(c.id)} set={(p) => savePrefs(c.id, p)} />)}</ul>
      {!list.length ? <p className="zm-empty">{search ? 'Nenhuma conversa encontrada.' : 'As conversas do seu celular aparecem aqui.'}</p> : null}
      <footer className="zm-side-foot"><Avatar name={name} size={32} /><span><b>{name}</b><span className={connected ? 'zm-online' : 'zm-offline'} role="status">{status}</span></span>{footer}</footer>
    </aside>
    <main className="zm-main">
      {children ?? (conversation ? <>
        <header className="zm-chat-head"><span className="zm-title-pill glass-pill"><Avatar name={conversation.title} size={28} /><b>{conversation.title}</b>{conversation.kind !== 'direct' ? <span>{conversation.members} pessoas</span> : null}</span></header>
        <section className="zm-timeline" role="log" aria-label="Mensagens" aria-live="polite">
          {conversation.waiting ? <p className="zm-note">Seu celular está preparando esta conversa para o navegador. Deixe ele conectado.</p> : null}
          {conversation.unreadable ? <p className="zm-note">{conversation.unreadable === 1 ? 'Uma mensagem anterior está' : `${conversation.unreadable} mensagens anteriores estão`} só no seu celular.</p> : null}
          {bubbles(timeline, false)}
          <div ref={end} />
        </section>
        <Composer key={conversation.id} placeholder={connected ? 'Mensagem' : 'Reconectando…'} disabled={!connected || !conversation.can_send} reply={reply} editing={editing} autoFocus
          cancelReply={() => { setReply(null); setEditing(null); }}
          attach={async (file) => { const m = messages.current; if (!m) throw new Error('Recarregue o Zoen.'); await m.sendFile(space, file); }}
          submit={async (text) => {
            const m = messages.current; if (!m) throw new Error('Recarregue o Zoen.');
            if (editing?.entry) { await m.annotate(space, editing.entry, { Edit: { text } }); setEditing(null); return; }
            await m.post({ space, text, ...(reply?.entry ? { replyTo: reply.entry } : {}) }); setReply(null);
          }} />
      </> : <section className="zm-blank"><h2>Suas conversas</h2><p>{status}</p></section>)}
    </main>
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
    {forward ? <div className="zm-scrim" onClick={() => setForward(null)}><div className="zm-sheet" role="dialog" aria-label="Encaminhar para" onClick={(e) => e.stopPropagation()}>
      <h2>Encaminhar para</h2>
      <ul>{conversations.filter((c) => c.can_send).map((c) => <li key={c.id}><button onClick={() => {
        const { text, file } = forward; setForward(null);
        void ((file ? messages.current?.forwardFile(c.id, file) : messages.current?.post({ space: c.id, text, forwarded: true }) ) ?? Promise.resolve()).then(() => select(c.id)).catch(fail);
      }}><Avatar name={c.title} size={32} />{c.title}</button></li>)}</ul>
      <button className="zm-pill" onClick={() => setForward(null)}>Cancelar</button>
    </div></div> : null}
    {toast ? <p className="zm-toast" role="alert">{toast}</p> : null}
  </div>;
}
