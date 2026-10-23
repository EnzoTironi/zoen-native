import { useEffect, useMemo, useRef, useState } from 'react';
import { message as reason } from './boundary.ts';
import type { Conversation, FileDoc, Message, Messages } from './messages.ts';
import { kindOf } from './media.tsx';
import { MessageEditor } from './message-editor.tsx';
import { MessageSearchResults } from './message-search-view.tsx';
import type { MessageHit } from './message-search.ts';
import { Bubble } from './messenger-bubble.tsx';
import { Composer } from './messenger-composer.tsx';
import { Avatar, day, Icon } from './messenger-presentation.tsx';

import { loadChatPreferences, saveChatPreferences, type ChatPreferences as Prefs } from './chat-preferences.ts';

export { Bubble } from './messenger-bubble.tsx';

/** How I keep a chat. Local to this account/browser until synced preferences reach the web core. */
function loadPrefs(identity: string): Record<string, Prefs> {
  try { return loadChatPreferences(localStorage, identity); } catch { return {}; }
}
const FOLLOW = 'zoen.thread-follow';
function loadFollow(): Record<string, boolean> { try { return JSON.parse(localStorage.getItem(FOLLOW) ?? '{}') as Record<string, boolean>; } catch { return {}; } }

const fileLabel = (f: FileDoc) => ({ image: 'Foto', video: 'Vídeo', audio: 'Mensagem de voz', file: f.name } as const)[kindOf(f.mime)];

function ChatRow({ c, active, prefs, open, set }: { c: Conversation; active: boolean; prefs: Prefs; open: () => void; set: (p: Prefs) => void }) {
  const last = [...c.messages].reverse().find((m) => !m.thread);
  const [menu, setMenu] = useState(false);
  const preview = last ? (last.deleted ? 'Mensagem apagada' : `${c.kind !== 'direct' && last.name ? `${last.name}: ` : ''}${last.file ? fileLabel(last.file) : last.text}`) : c.waiting ? 'Preparando a conversa…' : 'Nenhuma mensagem ainda';
  return <li data-space-id={c.id} className={`zm-chat${active ? ' zm-active' : ''}${c.unread ? ' zm-unread' : ''}`} onContextMenu={(e) => { e.preventDefault(); setMenu(true); }} onMouseLeave={() => setMenu(false)}>
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

type MessengerProps = {
  conversations: Conversation[]; identity: string; name: string; connected: boolean; status: string;
  messages: React.RefObject<Messages | null>; footer: React.ReactNode; selected: string; select: (id: string) => void; children?: React.ReactNode;
};

export function Messenger(props: MessengerProps) {
  // Account switches must also discard drafts, selections and in-memory preferences.
  return <AccountMessenger key={props.identity} {...props} />;
}

function AccountMessenger({ conversations, identity, name, connected, status, messages, footer, selected, select, children }: MessengerProps) {
  const [prefs, setPrefs] = useState(() => loadPrefs(identity));
  const [follow, setFollow] = useState(loadFollow);
  const [search, setSearch] = useState('');
  const [showArchived, setShowArchived] = useState(false);
  const [reply, setReply] = useState<Message | null>(null);
  const [editing, setEditing] = useState<Message | null>(null);
  const [threadRoot, setThreadRoot] = useState<string | null>(null);
  const [forward, setForward] = useState<Message | null>(null);
  const [toast, setToast] = useState('');
  const end = useRef<HTMLDivElement>(null);
  const [searchTarget, setSearchTarget] = useState<MessageHit | null>(null);
  const savePrefs = (id: string, p: Prefs) => {
    const next = { ...prefs, [id]: p }; setPrefs(next);
    try { saveChatPreferences(localStorage, identity, next); } catch { /* Storage may be disabled. */ }
  };
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
      <label className="zm-search"><Icon name="search" /><span className="zw-sr-only">Buscar conversas e mensagens</span>
        <input type="search" placeholder="Buscar" value={search} onChange={(e) => setSearch(e.target.value)} /></label>
      {!showArchived && threadsUnread ? <p className="zm-threads-row"><Icon name="thread" />Threads que você segue<span className="zm-badge">{threadsUnread}</span></p> : null}
      {!showArchived && archived.length ? <button className="zm-archived-row" onClick={() => setShowArchived(true)}><Icon name="archive" />Arquivadas<span>{archived.length}</span></button> : null}
      {search.trim() ? <MessageSearchResults key={search.trim()} conversations={conversations} query={search} open={(hit) => {
        select(hit.space); setSearchTarget(hit);
      }} /> : null}
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
        <Composer key={conversation.id} placeholder={connected ? 'Mensagem' : 'Reconectando…'} disabled={!connected || !conversation.can_send} reply={reply} autoFocus
          cancelReply={() => { setReply(null); setEditing(null); }}
          attach={async (file) => { const m = messages.current; if (!m) throw new Error('Recarregue o Zoen.'); await m.sendFile(space, file); if (file.voice?.reply) setReply(null); }}
          submit={async (text) => {
            const m = messages.current; if (!m) throw new Error('Recarregue o Zoen.');
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
      }}><Avatar name={c.title} size={32} />{c.title}</button></li>)}</ul>
      <button className="zm-pill" onClick={() => setForward(null)}>Cancelar</button>
    </div></div> : null}
    {toast ? <p className="zm-toast" role="alert">{toast}</p> : null}
  </div>;
}
