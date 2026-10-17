import { NativeAvatar } from './native-art';
import type { NativeAvatarIdentity } from './native-art';
import './native-navigation.css';

export type NativeNavigationTab = 'chats' | 'spaces' | 'notifications' | 'marketplace' | 'profile';
export type NativeCreateDestination = 'zoen' | 'agents' | 'files' | 'you';
/** Legacy contract retained for callers; recording belongs to the detail composer. */
export interface NativeNavigationVoice {
  active: boolean; locked: boolean; seconds: number; levels: readonly number[];
  start: () => void; lock: () => void; send: () => void; cancel: () => void;
}
export interface NativeNavigationProps {
  selected: NativeNavigationTab | 'store';
  onSelect: (tab: NativeNavigationTab) => void;
  onNewMessage?: () => void;
  notificationCount?: number;
  offline?: boolean;
  identity?: NativeAvatarIdentity;
  /** @deprecated Accepted only during migration; no search/fan/voice UI is rendered. */
  search?: { open: boolean; value: string; onChange: (value: string) => void; onOpenChange: (open: boolean) => void };
  onCreate?: (destination: NativeCreateDestination | 'new-chat') => void;
  unreadCount?: number;
  voice?: NativeNavigationVoice;
}

/** Filled vector approximations of the system tab symbols shown by the native TabView.
 * SF Symbols are not exported web assets; these are not claimed as exact font outlines. */
function TabSymbol({ name }: { name: Exclude<NativeNavigationTab, 'profile'> }) {
  return <svg className="zn-symbol" viewBox="0 0 28 28" fill="currentColor" aria-hidden="true">
    {name === 'chats' ? <><path d="M2 2h15a4 4 0 0 1 4 4v3H15a5 5 0 0 0-5 5v7l-4-4H5a4 4 0 0 1-4-4V6a4 4 0 0 1 1-4Z"/><path d="M16 10h8a3 3 0 0 1 3 3v8a3 3 0 0 1-3 3h-1v4l-5-4h-3a3 3 0 0 1-3-3v-7a4 4 0 0 1 4-4Z"/></> : name === 'marketplace' ? <><path d="M3 4h22l2 8H1l2-8Zm0 10h3v11h16V14h3v13H3V14Z"/><path d="M10 16h8v9h-8Z"/></> : name === 'spaces' ? <><rect x="3" y="3" width="10" height="10" rx="2"/><rect x="15" y="3" width="10" height="10" rx="2"/><rect x="3" y="15" width="10" height="10" rx="2"/><rect x="15" y="15" width="10" height="10" rx="2"/></> : <><path d="M12 2a2 2 0 0 1 4 0c4 .8 6 4 6 8v6l3 5a1 1 0 0 1-1 2H4a1 1 0 0 1-1-2l3-5v-6c0-4 2-7.2 6-8Z"/><path d="M10 25h8a4 4 0 0 1-8 0Z"/></>}
  </svg>;
}
const tabs = [{ id: 'chats', label: 'Chats' }, { id: 'marketplace', label: 'Marketplace' }, { id: 'profile', label: 'Profile' }] as const;

export function NativeNavigation({ selected, onSelect, onNewMessage, onCreate, identity, offline = false }: NativeNavigationProps) {
  return <nav className="zn-navigation" aria-label="Main navigation">
    <div className="zn-tabs">{tabs.map(({ id, label }) => <button type="button" key={id} className="zn-tab" aria-current={(selected === id || selected === 'store' && id === 'marketplace') ? 'page' : undefined} aria-label={label} onClick={() => onSelect(id)}>
      <span className="zn-tab-icon">{id === 'profile' ? identity ? <NativeAvatar identity={identity} size={26} animate={false}/> : <svg className="zn-symbol" viewBox="0 0 28 28" aria-hidden="true" fill="currentColor"><circle cx="14" cy="9" r="5"/><path d="M4 26v-3a10 10 0 0 1 20 0v3Z"/></svg> : <TabSymbol name={id}/>}</span>
      <span className="zn-label">{label}</span>
    </button>)}</div>
    {offline ? <span className="zn-offline-dot" role="status" aria-label="Offline" /> : null}
    <button type="button" className="zn-new-message" aria-label="Search" onClick={() => { if (onNewMessage) onNewMessage(); else onCreate?.('new-chat'); }}><svg viewBox="0 0 28 28" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><circle cx="12" cy="12" r="8"/><path d="m18 18 7 7"/></svg></button>
  </nav>;
}
