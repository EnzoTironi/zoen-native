import { useEffect, useRef, useState } from 'react';
import type { CSSProperties } from 'react';
import './native-navigation.css';

export type NativeNavigationTab = 'chats' | 'store';
export type NativeCreateDestination = 'zoen' | 'agents' | 'files' | 'you';
export interface NativeNavigationProps {
  selected: NativeNavigationTab;
  /** Controlled inbox search. Opening and cancelling also notify the host. */
  search: { open: boolean; value: string; onChange: (value: string) => void; onOpenChange: (open: boolean) => void };
  onSelect: (tab: NativeNavigationTab) => void;
  onCreate: (destination: NativeCreateDestination) => void;
  unreadCount?: number;
}

type Glyph = NativeNavigationTab | NativeCreateDestination | 'search' | 'plus' | 'close';
/** Native ZoenGlyph's 24-unit vector geometry, not platform font symbols. */
function NativeGlyph({ name }: { name: Glyph }) {
  const paths: Record<Glyph, React.ReactNode> = {
    chats: <><path d="M3 5.5V4.2L4.2 3h10.6L16 4.2v7.6L14.8 13H8l-3.5 3 .5-3h-.8L3 11.8Z" /><rect x="9" y="9" width="12.5" height="9.5" rx="3" /><path d="m18 18.5 1.5 2.5-4-2.5" /></>,
    store: <><path d="M4.2 4h15.6L21 9l-3.5 1.2-3-1.2-2.5 1.2L9.5 9l-3 1.2L3 9Z M4.5 10.5v10h15v-10 M10 20.5V15h4v5.5" /></>,
    search: <><circle cx="10.5" cy="10.5" r="6.6" /><path d="m15.4 15.4 5.2 5.2" /></>,
    plus: <path d="M12 4.8v14.4M4.8 12h14.4" />,
    close: <path d="m6 6 12 12M18 6 6 18" />,
    zoen: <path d="m12 2.8 1.6 6.8 7 2-7 2-1.6 7.4-1.6-7.4-7-2 7-2ZM19 2.8v3.4m-1.7-1.7h3.4" />,
    agents: <><circle cx="10" cy="8.4" r="3.6" /><path d="M3.6 20.2Q4.4 13.8 10 13.8t6.4 6.4M19.4 3.4v4.8M17 5.8h4.8" /></>,
    files: <path d="M3 6.4 3.6 5h5.6l2 2.6h9.2L21 9v9.6l-1 1H4l-1-1ZM3.2 10.4h17.6" />,
    you: <><circle cx="12" cy="12" r="9.4" /><circle cx="12" cy="10" r="3.2" /><path d="M6.4 18.6Q12 11 17.6 18.6" /></>,
  };
  return <svg width="23" height="23" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">{paths[name]}</svg>;
}

const destinations: { id: NativeCreateDestination; title: string }[] = [
  { id: 'zoen', title: 'Ask Zoen' }, { id: 'agents', title: 'Your agents' },
  { id: 'files', title: 'Files' }, { id: 'you', title: 'Your context' },
];

export function NativeNavigation({ selected, search, onSelect, onCreate, unreadCount = 0 }: NativeNavigationProps) {
  const [expanded, setExpanded] = useState(false);
  const trigger = useRef<HTMLButtonElement>(null);
  const input = useRef<HTMLInputElement>(null);
  useEffect(() => { if (search.open) { setExpanded(false); input.current?.focus(); } }, [search.open]);
  useEffect(() => {
    if (!expanded) return;
    const close = (event: KeyboardEvent) => { if (event.key === 'Escape') { setExpanded(false); trigger.current?.focus(); } };
    document.addEventListener('keydown', close);
    return () => document.removeEventListener('keydown', close);
  }, [expanded]);
  const cancelSearch = () => { search.onChange(''); search.onOpenChange(false); };
  return <nav className="zn-navigation" aria-label="Main navigation">
    {expanded && <button className="zn-fan-backdrop" aria-label="Close create menu" onClick={() => setExpanded(false)} />}
    {search.open ? <>
      <div className="zn-search zn-glass"><NativeGlyph name="search" /><input ref={input} type="search" aria-label="Search" placeholder="Search" value={search.value} onChange={event => search.onChange(event.target.value)} onKeyDown={event => { if (event.key === 'Escape') cancelSearch(); }} />{search.value && <button aria-label="Clear search" onClick={() => search.onChange('')}><NativeGlyph name="close" /></button>}</div>
      <button className="zn-cancel zn-glass" onClick={cancelSearch}>Cancel</button>
    </> : <>
      <button className="zn-circle zn-glass" aria-label="Search" onClick={() => search.onOpenChange(true)}><NativeGlyph name="search" /></button>
      <div className="zn-tabs zn-glass">{(['chats', 'store'] as const).map(tab => <button key={tab} className="zn-tab" aria-current={selected === tab ? 'page' : undefined} aria-label={tab === 'chats' && unreadCount > 0 ? `Chats, ${unreadCount} unread` : tab === 'chats' ? 'Chats' : 'Store'} onClick={() => onSelect(tab)}><span className="zn-tab-icon"><NativeGlyph name={tab} />{tab === 'chats' && unreadCount > 0 && <span className="zn-badge">{Math.min(unreadCount, 99)}</span>}</span>{selected === tab && <span>{tab === 'chats' ? 'Chats' : 'Store'}</span>}</button>)}</div>
      <div className="zn-create">
        {expanded && <div className="zn-fan" aria-label="Create destinations">{destinations.map((item, index) => {
          const angle = (90 + index * 30) * Math.PI / 180;
          const style = { '--fan-x': `${Math.cos(angle) * 107}px`, '--fan-y': `${-Math.sin(angle) * 107}px`, '--fan-index': index } as CSSProperties;
          return <button key={item.id} className="zn-fan-item zn-circle zn-glass" style={style} aria-label={item.title} onClick={() => { setExpanded(false); onCreate(item.id); trigger.current?.focus(); }}><NativeGlyph name={item.id} /><span className="zn-fan-label">{item.title}</span></button>;
        })}</div>}
        <button ref={trigger} className="zn-circle zn-plus" aria-label={expanded ? 'Close create menu' : 'Open create menu'} aria-expanded={expanded} onClick={() => setExpanded(value => !value)}><NativeGlyph name="plus" /></button>
      </div>
    </>}
  </nav>;
}
