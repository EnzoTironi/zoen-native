import { useState } from 'react';
import './chat-inbox-search.css';

/** Mirrors the native ChatInboxSearchBar; filtering remains owned by each inbox. */
export function ChatInboxSearch({ query, onQuery, filter, onFilter, onFocusChange }: { query: string; onQuery: (value: string) => void; filter: string; onFilter: (value: string) => void; onFocusChange?: (focused: boolean) => void }) {
  const [focused, setFocused] = useState(false);
  return <div className="chat-inbox-search" data-compact={focused || !!query}>
    <svg viewBox="0 0 24 24" aria-hidden="true"><circle cx="10" cy="10" r="7"/><path d="m15 15 6 6"/></svg>
    <input type="search" aria-label="Search chats" placeholder="Search chats" value={query} onChange={event => onQuery(event.target.value)} onFocus={() => { setFocused(true); onFocusChange?.(true); }} onBlur={() => { setFocused(false); onFocusChange?.(false); }}/>
    {query && <button aria-label="Clear search" onClick={() => onQuery('')}>×</button>}
    <span className="chat-search-divider" aria-hidden="true"/>
    <label className="chat-search-filter"><svg viewBox="0 0 24 24" aria-hidden="true"><path d="M3 6h18M6 12h12M9 18h6"/></svg><select aria-label="Filter chats" value={filter} onChange={event => onFilter(event.target.value)}><option value="all">All chats</option><option value="direct">Direct chats</option><option value="group">Groups</option><option value="community">Communities</option></select></label>
  </div>;
}
