import { useEffect, useState, type ReactNode } from 'react';
import type fixture from './fixtures/native-parity-v1.json';
import './native-adaptive.css';

export type NativeAdaptiveSpace = typeof fixture.spaces[number];
export type NativeAdaptiveDestination = 'chats' | 'activity' | 'store' | 'files' | 'agents' | 'you' | 'search' | 'new-chat' | 'participants' | 'zoen';
export interface NativeAdaptiveShellProps {
  children: ReactNode;
  spaces: readonly NativeAdaptiveSpace[];
  /** Space ID or rail destination. Null selects Paraty, then the first chat. */
  selected: string | null;
  onSelect: (selection: string) => void;
}
const tabs = ['chats', 'activity', 'store', 'files', 'agents', 'you', 'search'] as const;
const titles: Record<string, string> = { chats: 'Chats', activity: 'Activity', store: 'Store', files: 'Files', agents: 'Agents', you: 'You', search: 'Search' };
function Icon({ name }: { name: string }) {
  const paths: Record<string, ReactNode> = {
    chats: <><path d="M4 4h13v10H9l-5 4Z"/><path d="M17 8h4v11l-4-3h-4"/></>,
    activity: <><path d="M6 16V9a6 6 0 0 1 12 0v7l2 2H4Z"/><path d="M10 21h4"/></>,
    store: <><path d="m3 9 2-5h14l2 5M4 10v11h16V10M3 9q3 4 6 0 3 4 6 0 3 4 6 0M10 21v-6h4v6"/></>,
    files: <path d="M3 6h7l2 3h9v11H3Z"/>,
    agents: <><circle cx="9" cy="8" r="3"/><path d="M2 20v-2a7 7 0 0 1 14 0v2M16 5a3 3 0 0 1 0 6M18 14q4 1 4 6"/></>,
    you: <><circle cx="12" cy="12" r="10"/><circle cx="12" cy="9" r="3"/><path d="M5 19q7-9 14 0"/></>,
    search: <><circle cx="10" cy="10" r="7"/><path d="m15 15 6 6"/></>,
    sidebar: <><rect x="3" y="4" width="18" height="16" rx="2"/><path d="M9 4v16"/></>,
    plus: <path d="M12 4v16M4 12h16"/>,
    edit: <><path d="M10 5H4v16h16v-6M9 15l1-5L19 1l4 4-9 9Z"/></>,
    info: <><circle cx="12" cy="12" r="10"/><path d="M12 11v6M12 7h.01"/></>,
    filter: <path d="M3 6h18M7 12h10M10 18h4"/>,
    sun: <><circle cx="12" cy="12" r="4"/><path d="M12 1v3M12 20v3M1 12h3M20 12h3M4 4l2 2M18 18l2 2M4 20l2-2M18 6l2-2"/></>,
  };
  return <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">{paths[name] ?? paths.plus}</svg>;
}
export function NativeAdaptiveShell({ children, spaces, selected, onSelect }: NativeAdaptiveShellProps) {
  const [expanded, setExpanded] = useState(true);
  const [query, setQuery] = useState('');
  const [filter, setFilter] = useState('all');
  const [fan, setFan] = useState(false);
  const [appearance, setAppearance] = useState('system');
  const first = spaces.find(space => space.title === 'Paraty') ?? spaces[0];
  const active = selected ?? first?.id;
  const activeTab = tabs.find(tab => tab === active) ?? 'chats';
  useEffect(() => { if (selected == null && first) onSelect(first.id); }, [selected, first?.id, onSelect]);
  useEffect(() => { setQuery(''); }, [selected]);
  return <div className={`na-shell na-${appearance}`} data-sidebar-expanded={expanded} onKeyDown={event => { if (event.key === 'Escape') setFan(false); }}>
    <aside className="na-sidebar"><nav className="na-rail" aria-label="Main navigation">
      <button className="na-toggle" aria-label={expanded ? 'Hide sidebar' : 'Show sidebar'} aria-expanded={expanded} onClick={() => setExpanded(!expanded)}><Icon name="sidebar"/></button>
      {tabs.map(tab => <button key={tab} title={titles[tab]} aria-label={titles[tab]} aria-current={activeTab === tab ? 'page' : undefined} onClick={() => onSelect(tab)}><Icon name={tab}/>{tab === 'chats' && spaces.some(s => s.unread > 0) && <span className="na-badge">{Math.min(99, spaces.reduce((sum, space) => sum + space.unread, 0))}</span>}</button>)}
      <div className="na-rail-spacer"/>
      <label className="na-appearance" title="Appearance"><Icon name="sun"/><select aria-label="Appearance" value={appearance} onChange={e => setAppearance(e.target.value)}><option value="system">System</option><option value="light">Light</option><option value="dark">Dark</option></select></label>
      <div className="na-create">{fan && <div className="na-fan">{[['zoen','Ask Zoen'],['agents','Your agents'],['files','Files'],['you','Your context']].map(([id,title]) => <button key={id} onClick={() => { setFan(false); onSelect(id!); }}>{title}</button>)}</div>}<button className="na-plus" aria-label="Open create menu" aria-expanded={fan} onClick={() => setFan(!fan)}><Icon name="plus"/></button></div>
    </nav>{expanded && <section className="na-chat-sidebar" aria-label="Chats"><header><b>Chats</b><label className="na-filter"><Icon name="filter"/><select aria-label="Filter chats" value={filter} onChange={e => setFilter(e.target.value)}><option value="all">All chats</option><option value="direct">Direct chats</option><option value="group">Groups</option><option value="community">Communities</option></select></label><button aria-label="Participants" onClick={() => onSelect('participants')}><Icon name="info"/></button><button aria-label="New Chat" onClick={() => onSelect('new-chat')}><Icon name="edit"/></button></header>
      <label className="na-search"><Icon name="search"/><input aria-label="Search" placeholder="Search" value={query} onChange={e => setQuery(e.target.value)}/>{query && <button aria-label="Clear search" onClick={() => setQuery('')}>×</button>}</label>
      <div className="na-list">{spaces.filter(s => (filter === 'all' || s.kind === filter) && s.title.toLocaleLowerCase().includes(query.toLocaleLowerCase())).map(space => <button className="na-chat-row" key={space.id} aria-current={active === space.id ? 'page' : undefined} onClick={() => onSelect(space.id)}><span className="na-avatar" style={{ background: space.counterpart?.tintHex ?? '#19B5A5' }}>{space.counterpart?.initials ?? space.title.split(' ').slice(0,2).map(w => w[0]).join('')}</span><span className="na-copy"><span><b>{space.title}</b><time>{new Intl.DateTimeFormat('en-US',{hour:'numeric',minute:'2-digit',timeZone:'UTC'}).format(space.lastAtMs)}</time></span><small>{space.lastPreview}</small></span>{space.unread > 0 && <span className="na-row-badge">{space.unread}</span>}</button>)}</div>
    </section>}</aside><main className="na-detail">{children}</main>
  </div>;
}
