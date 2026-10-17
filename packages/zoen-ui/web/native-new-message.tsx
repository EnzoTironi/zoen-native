import type { Conversation } from './messages.ts';
import type { MessageHit } from './message-search.ts';
import { MessageSearchResults } from './message-search-view.tsx';
import React, { useEffect, useRef, useState } from 'react';
import { NativeAvatar } from './native-art.js';
import type { NativeRootItem, NativeRootPersona } from './native-root-screens.js';
import './native-new-message.css';

export type NativeNewMessageProps = {
  contacts: NativeRootPersona[] | null;
  conversations?: Conversation[];
  items?: NativeRootItem[] | null;
  onOpenConversation?: (id: string) => void;
  onOpenMessage?: (hit: MessageHit) => void;
  onClose: () => void;
  /** Opens a known conversation; this component does not create a direct chat. */
  onSelect?: (personaId: string) => void;
  /** Opens the client's supported invitation flow, not a simulated join. */
  onInvite?: () => void;
  online?: boolean;
};
export function filterNewMessageContacts(contacts: NativeRootPersona[], query: string) {
  const q = query.trim().replace(/^@+/, '').toLocaleLowerCase();
  return q.length < 2 ? [] : contacts.filter(p => !p.isMine && p.kind === 'person' &&
    (p.name.toLocaleLowerCase().includes(q) || p.handle?.toLocaleLowerCase().includes(q)));
}
/** Shared production sheet, following Shared/Sync/NewChatSheet.swift. */
export function NativeNewMessage({ contacts, onClose, onSelect, onInvite, online, conversations, onOpenConversation, onOpenMessage, items }: NativeNewMessageProps) {
  const [mode, setMode] = useState<'Chat' | 'Group' | 'Community' | 'Join'>('Chat');
  const [query, setQuery] = useState('');
  const sheet = useRef<HTMLElement>(null);
  useEffect(() => {
    const previous = document.activeElement;
    sheet.current?.focus();
    const key = (event: KeyboardEvent) => {
      if (event.key === 'Escape') { event.preventDefault(); onClose(); }
      if (event.key === 'Tab') {
        const nodes = Array.from(sheet.current?.querySelectorAll<HTMLElement>('button:not(:disabled), input:not(:disabled)') ?? []);
        const first = nodes[0], last = nodes[nodes.length - 1];
        if (event.shiftKey && (document.activeElement === first || document.activeElement === sheet.current)) { event.preventDefault(); last?.focus(); }
        else if (!event.shiftKey && document.activeElement === last) { event.preventDefault(); first?.focus(); }
      }
    };
    document.addEventListener('keydown', key);
    return () => { document.removeEventListener('keydown', key); if (previous instanceof HTMLElement) previous.focus(); };
  }, [onClose]);
  const results = conversations && !query.trim() ? (contacts ?? []).filter(person => !person.isMine && person.kind === 'person') : filterNewMessageContacts(contacts ?? [], query);
  return <div className="nnm-backdrop"><section className="native-new-message" ref={sheet} tabIndex={-1} role="dialog" aria-modal="true" aria-labelledby="nnm-title">
    <header><button className="nnm-cancel" onClick={onClose}>Cancel</button><h1 id="nnm-title">{mode === 'Join' ? 'Join a chat' : conversations ? 'Search' : 'New chat'}</h1>{(mode === 'Group' || mode === 'Community') && <button className="nnm-create" disabled>Create</button>}</header>
    <div className="nnm-form">{conversations && <>
      <label className="nnm-field"><input id="unified-search-field" type="search" aria-label="Search chats, messages and people" placeholder="Chats, messages, or @handle" value={query} onChange={e => setQuery(e.target.value)}/></label>
      <section aria-label="Chats"><h2>Chats</h2>{conversations.filter(chat => chat.title.toLocaleLowerCase().includes(query.trim().toLocaleLowerCase())).map(chat => <button className="nnm-person" key={chat.id} onClick={() => onOpenConversation?.(chat.id)}>{chat.title}</button>)}</section>
      {query.trim() && onOpenMessage && <MessageSearchResults key={query} conversations={conversations} query={query} open={onOpenMessage}/>}
      {query.trim() && <section aria-label="Items"><h2>Items</h2>{items == null ? <p className="nnm-status">Item search is unavailable on this client.</p> : items.filter(item => (item.title + ' ' + (item.text ?? '')).toLocaleLowerCase().includes(query.trim().toLocaleLowerCase())).map(item => <article key={item.id}><strong>{item.title}</strong><p>{item.text}</p><small>{item.spaceTitle} · Open item details in the native app.</small></article>)}</section>}
      <h2>People</h2>
    </>}{conversations && <details className="nnm-create-menu"><summary>Create or join</summary><div className="nnm-modes" role="tablist" aria-label="Conversation type">{(['Chat', 'Group', 'Community', 'Join'] as const).map(label => <button key={label} role="tab" aria-selected={mode === label} onClick={() => setMode(label)}>{label}</button>)}</div></details>}{!conversations && <div className="nnm-modes" role="tablist" aria-label="Conversation type">{(['Chat', 'Group', 'Community', 'Join'] as const).map(label => <button key={label} role="tab" aria-selected={mode === label} onClick={() => setMode(label)}>{label}</button>)}</div>}
      {online === false && <div className="nnm-offline"><svg viewBox="0 0 24 24" aria-hidden="true" fill="none" stroke="currentColor" strokeWidth="2"><path d="m3 3 18 18M2 8c5-5 15-5 20 0M5 12c3-3 7-3 10-1M9 16c2-2 4-2 6 0m-4 4h2"/></svg><span>You’re offline. Finding people and invites need a connection; chats you start still go out when you’re back.</span></div>}
      {(mode === 'Chat' || mode === 'Group') ? <>
        {mode === 'Group' && <><div className="nnm-field"><input placeholder="Group name" disabled aria-label="Group name"/></div><p className="nnm-status">Creating groups is unavailable on this client.</p></>}
        <div className="nnm-people">{!conversations && <label className="nnm-field"><span aria-hidden="true">@</span><input aria-label="Search by @handle" placeholder="Search by @handle" value={query} onChange={e => setQuery(e.target.value)} autoCapitalize="none" autoCorrect="off"/></label>}
          {results.map(person => <button className="nnm-person" key={person.id} disabled={mode !== 'Chat' || !onSelect} onClick={() => onSelect?.(person.id)}><NativeAvatar identity={person} size={36}/><span><strong>{person.name}</strong>{person.handle && <small>@{person.handle}</small>}</span></button>)}
        </div>
        {contacts === null ? <p className="nnm-status" role="status">People search is unavailable on this client.</p> : query.trim().replace(/^@+/, '').length >= 2 && results.length === 0 ? <p className="nnm-status" role="status">No matching known contacts. Relay people search is unavailable on this client.</p> : null}
        {!onSelect && results.length > 0 && <p className="nnm-status">Opening these conversations is unavailable on this client.</p>}
      </> : mode === 'Community' ? <><div className="nnm-field"><input placeholder="Community name" aria-label="Community name" disabled/></div><p className="nnm-status">A conversation for a club or neighbourhood, alongside your other chats.</p><p className="nnm-status">Creating communities is unavailable on this client.</p></> : <><button className="nnm-invite" disabled={!onInvite} onClick={onInvite}>Invite code</button>{!onInvite && <p className="nnm-status">Joining with an invite is unavailable on this client.</p>}</>}
    </div>
  </section></div>;
}
