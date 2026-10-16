import { useEffect, useLayoutEffect, useRef, useState, type ReactNode } from 'react';
import './living-space.css';
import { LivingConfirmation } from './living-confirmation.tsx';

export interface SpacePage { id: string; title: string; thumbnail?: string }
export interface SpaceAnnouncement { id: string; title: string; body: string }
export interface SpaceJoinRequest { id: string; name: string; spaceTitle: string; space?: string }
export interface LivingSpaceProps {
  title: string; members: number; coverUrl?: string; activeSpaceId?: string;
  announcements: readonly SpaceAnnouncement[]; pages: readonly SpacePage[]; availablePages?: readonly SpacePage[];
  canAnnounce: boolean; pinAnnouncement?: (id: string) => Promise<void>;
  openPage: (id: string) => void; createPage?: () => Promise<void>; choosePage?: (id: string) => Promise<void>;
  coverChoices?: readonly SpacePage[]; setCover?: (id: string) => Promise<void>;
  conversations: ReactNode; notifications?: ReactNode; hackathon?: ReactNode; tiers?: ReactNode; hackathonTitle?: string;
  conversationAvailable?: boolean; startConversation?: () => void; conversationRequest?: number;
  notificationCount?: number; openFolder?: () => void;
  spaces?: readonly SpacePage[]; selectSpace?: (id: string) => void;
  events?: readonly SpacePage[]; files?: readonly SpacePage[];
  detail?: ReactNode; detailTitle?: string; closeDetail?: () => boolean;
  openItem?: (id: string) => void; notice?: ReactNode;
}
export function livingSpacesEnabled(storage: Pick<Storage, 'getItem'> | undefined): boolean {
  try { return storage?.getItem('LivingSpacesEnabled') === 'true'; } catch { return false; }
}
type MarkName = 'home' | 'pin' | 'page' | 'calendar' | 'folder' | 'apps' | 'chat' | 'bell' | 'photo' | 'trophy' | 'member' | 'lock';
function Mark({ type }: { type: MarkName }) {
  const paths: Record<MarkName, string> = {
    home: 'm3 8 9-5 9 5-9 5-9-5Zm0 5 9 5 9-5M3 18l9 5 9-5',
    pin: 'M8 3h8l-1 6 3 3v2H6v-2l3-3-1-6ZM12 14v8',
    page: 'M5 3h9l5 5v13H5V3Zm9 0v6h5M8 13h8M8 17h6',
    calendar: 'M4 5h16v16H4V5Zm0 5h16M8 2v6M16 2v6',
    folder: 'M3 7V4h7l3 3h8v14H3V7Z',
    apps: 'M3 3h7v7H3V3Zm11 0h7v7h-7V3ZM3 14h7v7H3v-7Zm11 0h7v7h-7v-7Z',
    chat: 'M3 4h14v11H9l-6 4V4Zm15 4h3v13l-5-4h-3',
    bell: 'M5 17h14l-2-3V9a5 5 0 0 0-10 0v5l-2 3ZM10 21h4',
    photo: 'M3 4h18v16H3V4Zm0 12 5-5 5 5 3-3 5 5M16 8h.01',
    trophy: 'M7 3h10v7a5 5 0 0 1-10 0V3Zm0 2H3v3a4 4 0 0 0 4 4m10-7h4v3a4 4 0 0 1-4 4M12 15v6M8 21h8',
    lock: 'M7 10V7a5 5 0 0 1 10 0v3M5 10h14v11H5V10ZM12 14v3',
    member: 'M10 3a8 8 0 1 0 0 16M10 6a3 3 0 1 0 0 6M5 17a5 5 0 0 1 8-3M15 18l2 2 5-6',
  };
  return <svg viewBox="0 0 24 24" aria-hidden="true" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round"><path d={paths[type]} /></svg>;
}
function ConversationPane({ children }: { children: ReactNode }) {
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => { const log = ref.current?.querySelector('[role=log]'); if (log) log.scrollTop = log.scrollHeight; }, []);
  return <div className="zl-conversation" ref={ref}>{children}</div>;
}
export function LivingSpace({ title, members, coverUrl, activeSpaceId, announcements, pages, availablePages = pages, canAnnounce, pinAnnouncement, openPage, createPage, choosePage, coverChoices = [], setCover, conversations, notifications, hackathon, tiers, hackathonTitle = 'Hackathon', events = [], files = [], openItem, notice, conversationAvailable = true, startConversation, spaces = [], selectSpace, conversationRequest = 0, notificationCount = 0, openFolder, detail, detailTitle = '', closeDetail }: LivingSpaceProps) {
  const [section, setSection] = useState<'home' | 'spaces' | 'conversations' | 'notifications' | 'hackathon' | 'tiers'>('home');
  useEffect(() => { if (conversationRequest > 0) setSection('conversations'); }, [conversationRequest]);
  const [busy, setBusy] = useState(false); const locked = useRef(false);
  const [failure, setFailure] = useState('');
  const [showAnnouncements, setShowAnnouncements] = useState(false);
  const dialog = useRef<HTMLDialogElement>(null);
  useEffect(() => { if (showAnnouncements && dialog.current && !dialog.current.open) dialog.current.showModal(); }, [showAnnouncements]);
  async function run(operation: () => Promise<void>) {
    if (locked.current) return; locked.current = true; setBusy(true); setFailure('');
    try { await operation(); } catch (error) { setFailure(error instanceof Error ? error.message : 'Could not update this Space.'); }
    finally { locked.current = false; setBusy(false); }
  }
  const menu = (label: string, items: readonly SpacePage[], action: (id: string) => Promise<void>) => <details><summary aria-disabled={busy || items.length === 0}><Mark type={label === 'Set cover' ? 'photo' : label === 'Pin announcement' ? 'pin' : 'page'} />{label}</summary><div>{items.map((item) => <button key={item.id} type="button" disabled={busy} onClick={() => void run(() => action(item.id))}>{item.title || 'Untitled page'}</button>)}</div></details>;
  const hasDetail = detail !== undefined && detail !== null;
  const viewKey = hasDetail ? 'detail' : section;
  const main = useRef<HTMLElement>(null); const scrollPositions = useRef<Record<string, number>>({});
  useLayoutEffect(() => { if (main.current) main.current.scrollTop = scrollPositions.current[viewKey] ?? 0; }, [viewKey]);
  function go(next: typeof section) { if (!hasDetail || closeDetail?.() !== false) setSection(next); }
  return <div className="zl-space">
    <header className="zl-toolbar">{(hasDetail || section !== 'spaces' && section !== 'notifications') && <button className="zl-back" type="button" aria-label={hasDetail || section !== 'home' ? 'Back to Home' : 'Back to Spaces'} onClick={() => { if (hasDetail) closeDetail?.(); else setSection(section === 'home' ? 'spaces' : 'home'); }}>‹</button>}<strong>{hasDetail ? detailTitle : section === 'notifications' || section === 'spaces' ? '' : section === 'conversations' ? 'Conversations' : section === 'hackathon' ? 'Hackathon' : section === 'tiers' ? 'Memberships' : 'Home'}</strong></header>
    <nav className="zl-navigation" aria-label="Main navigation">
      <h1 className="zl-sidebar-title">Zoen</h1>
      <button type="button" aria-pressed={section !== 'notifications'} onClick={() => go('spaces')}><Mark type="home" /><span>Spaces</span></button>
      {notifications && <button type="button" aria-label="Notifications" aria-description={notificationCount ? `${notificationCount} pending requests` : undefined} aria-pressed={section === 'notifications'} onClick={() => go('notifications')}><Mark type="bell" /><span>Notifications{notificationCount > 0 ? ` (${notificationCount})` : ''}</span></button>}
      {section !== 'notifications' && spaces.length > 0 && <div className="zl-space-list">{spaces.map((space) => <button type="button" aria-current={space.id === activeSpaceId ? 'page' : undefined} key={space.id} onClick={() => { if (!hasDetail || closeDetail?.() !== false) { selectSpace?.(space.id); setSection('home'); } }}><Mark type="home" /><span>{space.title}</span></button>)}</div>}
    </nav>
    <main key={viewKey} ref={main} onScroll={(event) => { scrollPositions.current[viewKey] = event.currentTarget.scrollTop; }} className={`zl-scroll${section === 'conversations' ? ' zl-chat-detail' : ''}`}>
      {hasDetail ? detail : section === 'spaces' ? <div className="zl-home"><h1>Spaces</h1>{spaces.map((space) => <button className="zl-row zl-surface" type="button" key={space.id} onClick={() => { selectSpace?.(space.id); setSection('home'); }}>{space.title}</button>)}</div> : section === 'hackathon' ? hackathon : section === 'tiers' ? tiers : section === 'conversations' ? <ConversationPane>{conversations}</ConversationPane> : section === 'notifications' ? notifications : <div className="zl-home" data-testid="living-space-home">
        {notice && <div role="status" className="zl-secondary">{notice}</div>}
        <section className="zl-cover zl-surface">
          <div className="zl-cover-image">{coverUrl ? <img src={coverUrl} alt="" /> : <Mark type="photo" />}</div>
          <h1>{title}</h1><p>{members} {members === 1 ? 'member' : 'members'}</p>
          {canAnnounce && setCover && menu('Set cover', coverChoices, setCover)}
        </section>
        <section><h2><Mark type="pin" />Pinned announcements</h2>
          <button type="button" className="zl-announcement" onClick={() => setShowAnnouncements(true)}>View announcements</button>
          {announcements.length ? announcements.map((a) => <button type="button" key={a.id} className="zl-announcement" onClick={() => openPage(a.id)}>{a.title}</button>) : <p className="zl-secondary">No pinned announcements.</p>}
          {canAnnounce && pinAnnouncement && menu('Pin announcement', availablePages, pinAnnouncement)}
        </section>
        <section><h2><Mark type="page" />Live page</h2>
          {pages.length ? pages.map((page) => <button type="button" className="zl-row" key={page.id} onClick={() => openPage(page.id)}><Mark type="page" /><span>{page.title || 'Untitled page'}</span></button>) : <p className="zl-secondary">A shared page, edited together.</p>}
          {canAnnounce && choosePage && menu('Choose page', availablePages, choosePage)}
          {canAnnounce && createPage && <button type="button" className="zl-glass-button" disabled={busy} onClick={() => void run(createPage)}>Create page</button>}
        </section>
        {([{ title: 'Events', type: 'calendar', items: events, empty: 'No events yet.' }, { title: 'Files', type: 'folder', items: files, empty: 'No files yet.' }] as const).map(({ title, type, items, empty }) => <section key={type}><h2><Mark type={type} />{title}</h2>
          {items.length ? items.map((item) => <button type="button" className="zl-row" key={item.id} onClick={() => openItem?.(item.id)} disabled={!openItem}>{item.thumbnail ? <img className="zl-file-thumb" src={item.thumbnail} alt="" /> : type === 'folder' ? <Mark type="folder" /> : null}<span>{item.title}</span></button>) : <p className="zl-secondary">{empty}</p>}
          {type === 'folder' && openFolder && <button type="button" className="zl-announcement" onClick={openFolder}>Open folder</button>}
        </section>)}
        <section className="zl-slots" aria-label="Mini-apps"><div className="zl-frozen zl-surface"><Mark type="apps" /><span>Mini-apps</span><small>Coming later</small><Mark type="lock" /></div></section>
        {hackathon && <section><h2><Mark type="trophy" />Hackathon</h2><button type="button" className="zl-announcement" aria-label="Hackathon" onClick={() => setSection('hackathon')}>{hackathonTitle}</button></section>}
        {tiers && <section><h2><Mark type="member" />Memberships</h2><button type="button" className="zl-announcement" aria-label="Memberships" onClick={() => setSection('tiers')}>{canAnnounce ? 'Manage memberships' : 'View memberships'}</button></section>}
        <section><h2><Mark type="chat" />Conversations</h2><button type="button" className="zl-row" disabled={!conversationAvailable} onClick={() => { startConversation?.(); setSection('conversations'); }}><span>General conversation</span><span aria-hidden="true">›</span></button>{!conversationAvailable && <p className="zl-secondary">Connect your account to join the conversation.</p>}</section>
        <p className="zl-secondary zl-paused">Agents are paused in this Space.</p>
        {failure && <p role="alert">{failure}</p>}
      </div>}
    </main>
    {showAnnouncements && <dialog className="zl-announcements-dialog" ref={dialog} onClose={() => setShowAnnouncements(false)} onCancel={() => setShowAnnouncements(false)} aria-label="Announcements"><header className="zl-heading"><h1>Announcements</h1><button type="button" onClick={() => { dialog.current?.close(); setShowAnnouncements(false); }}>Done</button></header><div>{announcements.length === 0 ? <p>No pinned announcements.</p> : announcements.map((item) => <button type="button" key={item.id} onClick={() => { dialog.current?.close(); setShowAnnouncements(false); openPage(item.id); }}>{item.title}</button>)}</div></dialog>}
  </div>;
}
export function SpaceJoinNotifications({ requests, decide }: { requests: readonly SpaceJoinRequest[]; decide: (id: string, approve: boolean) => Promise<void> }) {
  const [pending, setPending] = useState<string | null>(null); const locked = useRef(false); const [failure, setFailure] = useState('');
  const [feedback, setFeedback] = useState({ revision: 0, label: '', positive: true });
  async function submit(id: string, approve: boolean) {
    if (locked.current) return; locked.current = true; setPending(id); setFailure('');
    try { await decide(id, approve); setFeedback((old) => ({ revision: old.revision + 1, label: approve ? 'Request approved' : 'Request declined', positive: approve })); }
    catch { setFailure('This request was not updated. Check your access and try again.'); }
    finally { locked.current = false; setPending(null); }
  }
  const groups = new Map<string, SpaceJoinRequest[]>();
  requests.forEach((request) => { const key = request.space ?? request.spaceTitle; const group = groups.get(key) ?? []; group.push(request); groups.set(key, group); });
  return <section className="zl-home zl-notifications" aria-label="Notifications"><h1>Notifications</h1><LivingConfirmation {...feedback} />{failure && <p role="alert">{failure}</p>}{requests.length === 0 && <p>No pending requests.</p>}{[...groups.entries()].map(([key, group]) => <section className="zl-notification-group" key={key}><h2>{group[0]?.spaceTitle}</h2>{group.map((request) => <article className="zl-notification-card" key={request.id}><h2><Mark type="member" />Join request</h2><p>{request.name}</p><div className="zl-notification-actions"><button aria-label="Approve" disabled={pending !== null} onClick={() => void submit(request.id, true)}>Approve</button><button aria-label="Decline" disabled={pending !== null} onClick={() => void submit(request.id, false)}>Decline</button></div></article>)}</section>)}</section>;
}
