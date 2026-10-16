import { useEffect, useRef, useState, type ReactNode } from 'react';
import './native-adaptive.css';
import { displayTime } from './display-clock.ts';
import { NativeAvatar, type NativeAvatarIdentity } from './native-art.tsx';

export interface NativeAdaptiveSpace {
  id: string;
  title: string;
  kind: string;
  lastAtMs: number;
  lastPreview: string;
  unread: number;
  counterpart: { tintHex: string; initials: string } | null;
}
export type NativeAdaptiveDestination = 'chats' | 'spaces' | 'notifications' | 'profile' | 'new-chat' | 'participants';
export interface NativeAdaptiveShellProps {
  children: ReactNode;
  /** Compatibility only: the current native shell has no account footer. */
  footer?: ReactNode;
  notificationCount?: number;
  identity?: NativeAvatarIdentity;
  spaces: readonly NativeAdaptiveSpace[];
  /** Space ID or rail destination. Null selects Paraty, then the first chat. */
  selected: string | null;
  onSelect: (selection: string) => void;
}
const tabs = ['chats', 'spaces', 'notifications', 'profile'] as const;
const titles: Record<string, string> = { chats: 'Chats', spaces: 'Spaces', notifications: 'Notifications', profile: 'Profile' };
function Icon({ name }: { name: string }) {
  const paths: Record<string, ReactNode> = {
    chats: <><path d="M4 4h13v10H9l-5 4Z"/><path d="M17 8h4v11l-4-3h-4"/></>,
    notifications: <><path d="M6 16V9a6 6 0 0 1 12 0v7l2 2H4Z"/><path d="M10 21h4"/></>,
    spaces: <><rect x="3" y="3" width="7" height="7" rx="1.5"/><rect x="14" y="3" width="7" height="7" rx="1.5"/><rect x="3" y="14" width="7" height="7" rx="1.5"/><rect x="14" y="14" width="7" height="7" rx="1.5"/></>,
    profile: <><circle cx="12" cy="8" r="4"/><path d="M4 22v-2a8 8 0 0 1 16 0v2"/></>,
    moon: <path d="M20 15A9 9 0 0 1 9 4a9 9 0 1 0 11 11Z"/>,
    system: <><rect x="2" y="3" width="20" height="14" rx="2"/><path d="M12 17v4M7 21h10"/></>,
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
export function NativeAdaptiveShell({ children, spaces, selected, onSelect, notificationCount = 0, identity }: NativeAdaptiveShellProps) {
  const [expanded, setExpanded] = useState(true);
  const [query, setQuery] = useState('');
  const [filter, setFilter] = useState('all');
  const lastChat = useRef<string | null>(null);
  const [appearance, setAppearance] = useState('system');
  const first = spaces.find(space => space.title === 'Paraty') ?? spaces[0];
  const active = selected ?? first?.id;
  const activeTab = tabs.find(tab => tab === active) ?? 'chats';
  useEffect(() => { if (selected == null && first) onSelect(first.id); }, [selected, first?.id, onSelect]);
  useEffect(() => { setQuery(''); }, [selected]);
  useEffect(() => { if (spaces.some(space => space.id === selected)) lastChat.current = selected; }, [selected, spaces]);
  function selectTab(tab: typeof tabs[number]) {
    setQuery('');
    if (tab === 'chats') {
      setExpanded(true);
      const remembered = spaces.find(space => space.id === lastChat.current);
      onSelect(remembered?.id ?? spaces[0]?.id ?? 'chats');
    } else onSelect(tab);
  }
  return <div className={`na-shell na-${appearance}`} data-sidebar-expanded={expanded} onKeyDown={event => { if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 'k') { event.preventDefault(); onSelect('new-chat'); } }}>
    <aside className="na-sidebar"><nav className="na-rail" aria-label="Main navigation">
      <button className="na-toggle" aria-label={expanded ? 'Hide sidebar' : 'Show sidebar'} aria-expanded={expanded} onClick={() => setExpanded(!expanded)}><Icon name="sidebar"/></button>
      {tabs.map(tab => {
        const badge = tab === 'notifications' ? notificationCount : tab === 'chats' ? spaces.reduce((sum, space) => sum + space.unread, 0) : 0;
        return <button key={tab} title={titles[tab]} aria-label={titles[tab]} aria-description={badge > 0 ? String(badge) : undefined} aria-current={activeTab === tab ? 'page' : undefined} onClick={() => selectTab(tab)}>{tab === 'profile' && identity ? <NativeAvatar identity={identity} size={26}/> : <Icon name={tab}/>}{badge > 0 && <span className="na-badge">{badge > 99 ? '99+' : badge}</span>}</button>;
      })}
      <div className="na-rail-spacer"/>
      <div className="na-appearance" role="group" aria-label="Appearance">{[-1, 0, 1].map(slot => {
        const modes = ['dark', 'system', 'light'];
        const mode = modes[(modes.indexOf(appearance) + slot + 3) % 3]!;
        return <button key={slot} aria-label={`${mode[0]!.toUpperCase()}${mode.slice(1)} appearance`} aria-pressed={slot === 0} onClick={() => setAppearance(mode)} onKeyDown={event => { if (event.key === 'ArrowLeft' || event.key === 'ArrowRight') { event.preventDefault(); setAppearance(modes[(modes.indexOf(appearance) + (event.key === 'ArrowLeft' ? 2 : 1)) % 3]!); } }}><Icon name={mode === 'dark' ? 'moon' : mode === 'light' ? 'sun' : 'system'}/></button>;
      })}</div>
      <button className="na-new-message" aria-label="New message" onClick={() => onSelect('new-chat')}><Icon name="edit"/></button>
    </nav>{expanded && <section className="na-chat-sidebar" aria-label="Chats"><header><b>Chats</b><label className="na-filter"><Icon name="filter"/><select aria-label="Filter chats" value={filter} onChange={e => setFilter(e.target.value)}><option value="all">All chats</option><option value="direct">Direct chats</option><option value="group">Groups</option><option value="community">Communities</option></select></label>{spaces.some(space => space.id === active) && <button aria-label="Participants" onClick={() => onSelect('participants')}><Icon name="info"/></button>}</header>
      <label className="na-search"><Icon name="search"/><input aria-label="Search" placeholder="Search" value={query} onChange={e => setQuery(e.target.value)}/>{query && <button aria-label="Clear search" onClick={() => setQuery('')}>×</button>}</label>
      <div className="na-list">{spaces.filter(s => (filter === 'all' || s.kind === filter) && s.title.toLocaleLowerCase().includes(query.toLocaleLowerCase())).map(space => <button className="na-chat-row" key={space.id} aria-current={active === space.id ? 'page' : undefined} onClick={() => onSelect(space.id)}><NativeAvatar className="na-avatar" size={34} identity={{ kind: space.kind === 'direct' ? 'person' : 'group', id: space.id, name: space.title, ...(space.counterpart ?? {}) }} /><span className="na-copy"><span><b>{space.title}</b><time>{displayTime(space.lastAtMs)}</time></span><small>{space.lastPreview}</small></span>{space.unread > 0 && <span className="na-row-badge">{space.unread}</span>}</button>)}</div>
    </section>}</aside><main className="na-detail">{children}</main>
  </div>;
}
