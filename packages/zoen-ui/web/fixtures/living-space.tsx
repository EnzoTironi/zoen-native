// Synthetic visual adapter. Production components, no relay/persistence/end-to-end claims.
import { useRef, useState } from 'react';
import { createRoot } from 'react-dom/client';
import { LivingSpace, SpaceJoinNotifications } from '../living-space.tsx';
import { Messenger } from '../messenger.tsx';
import { LivePage, type PageDto } from '../live-page.tsx';
import { Hackathon, CommunityTiers, type HackathonState, type HackathonCommand, type CommunityTier, type CommunityTierCommand } from '../hackathon.tsx';
import type { Conversation, Messages } from '../messages.ts';
import '../../src/tokens.css';
import '../web.css';
import '../messenger.css';
declare const ZOEN_LIVING_COVER: string;
const cover = ZOEN_LIVING_COVER;
const block = (id: string, text: string, kind = 'paragraph', level = 0) => ({ id, kind, level, indent: 0, number: 0, checked: false, lang: '', url: '', alt: '', text, spans: [] });
const conversation: Conversation = { id: 'garden', title: 'Living Space', kind: 'community', can_send: true, waiting: null, unreadable: 0, unread: 0, mentions: 0, members: 1, messages: [] };
function Fixture() {
  const [announcements, setAnnouncements] = useState([{ id: 'garden-page', title: 'Welcome to our Space', body: 'A shared place to plan, learn and stay connected.' }]);
  const [requests, setRequests] = useState<{ id: string; name: string; spaceTitle: string }[]>([{ id: 'fixture-join-request', name: 'Taylor', spaceTitle: 'Living Space' }]);
  const messages = useRef<Messages | null>(null);
  const [openedPage, setOpenedPage] = useState(false);
  const [page, setPage] = useState<PageDto>({ item_id: 'garden-page', space_id: 'garden', title: 'Welcome to our Space', path: 'welcome.md', version: 3, ready: true, unsaved: false, pending_sync: false, save_error: null, can_edit: true, content_hash: 'fixture-seeded-version-3', edit_context: 'fixture-context', blocks: [block('intro', 'Welcome to our Space', 'heading', 1), block('body', 'A shared place to plan, learn and stay connected.'), block('week', 'This week', 'heading', 2), block('ideas', 'Share your ideas', 'task'), block('gather', 'Plan our next gathering', 'task'), block('outro', 'Everyone can help keep this page up to date.')] });
  const [deadline] = useState(() => Date.now() + 100_000);
  const [hackathon, setHackathon] = useState<HackathonState>({ id: 'garden-build', title: 'Build together', deadline_ms: deadline, registered: ['living_builder'], profiles: {}, teams: [{ id: 'garden-builders', name: 'Garden builders', space: 'builders-chat', members: ['living_builder'], project_url: 'https://example.com/garden', files: ['cover'], votes: 0, score: 0 }], schedule: [{ title: 'Kickoff', at_ms: deadline - 3_600_000 }], winners: [], finalized: false });
  const [tiers, setTiers] = useState<CommunityTier[]>([{ id: 'builder', name: 'Builder', space: 'builder-partition', groups: ['builder-lounge'], resources: [{ id: 'builder-notes', kind: 'page' }], members: ['living_builder'] }]);
  async function hackathonCommand(command: HackathonCommand) {
    if (command.action === 'vote') setHackathon((h) => ({ ...h, teams: h.teams.map((t) => t.id === command.team_id ? { ...t, votes: t.votes + 1 } : t) }));
    else if (command.action === 'project_submit') setHackathon((h) => ({ ...h, teams: h.teams.map((t) => t.id === command.team_id ? { ...t, project_url: command.url, files: command.file_ids } : t) }));
    else throw Error('This synthetic fixture does not perform that core operation.');
  }
  async function tierCommand(command: CommunityTierCommand): Promise<void | string> {
    if (command.action === 'tier_create') setTiers((all) => [...all, { id: `fixture-${all.length}`, name: command.name, space: 'fixture-partition', groups: [], resources: [], members: [] }]);
    else throw Error('This synthetic fixture does not grant access or issue real invitations.');
  }
  // Version 3 is a synthetic input, not evidence of core save/reopen operations.
  const detail = openedPage ? <LivePage page={page} refresh={async () => page} save={async (request) => { const next = { ...page, blocks: request.blocks, version: page.version + 1, content_hash: `fixture-${page.version + 1}` }; setPage(next); return next; }} /> : undefined;
  return <><div className="fixture-shell" data-fixture-evidence="Offline controlled fixture; page version 3 seeded, not core save evidence"><main><LivingSpace detail={detail} detailTitle={page.title} closeDetail={() => { setOpenedPage(false); return true; }} title={conversation.title} members={1} coverUrl={cover} announcements={announcements} pages={[{ id: 'garden-page', title: 'Welcome to our Space' }]} canAnnounce
    notificationCount={requests.length} setCover={async () => {}} coverChoices={[{ id: 'cover', title: 'Space cover' }]} choosePage={async () => {}} createPage={async () => setOpenedPage(true)} events={[]} files={[{ id: 'cover', title: 'Space cover', thumbnail: cover }]} hackathonTitle="Build together" activeSpaceId="garden" spaces={[{ id: 'garden', title: 'Living Space' }, { id: 'coastal', title: 'Coastal Travelers' }]} selectSpace={() => {}}
    pinAnnouncement={async (id) => setAnnouncements((all) => all.some((a) => a.id === id) ? all : [...all, { id, title: page.title, body: '' }])}
    hackathon={<Hackathon hackathon={hackathon} identityId="living_builder" members={[{ id: 'living_builder', name: 'Alex' }]} isAdmin isJudge={false} availableFiles={[{ id: 'cover', title: 'Space cover' }]} command={hackathonCommand} openTeamConversation={async () => { throw Error('Fixture has no encrypted team conversation.'); }} demoSlot={<p>Demo mini-app · Coming later</p>} />}
    tiers={<CommunityTiers tiers={tiers} identityId="living_builder" isAdmin currentSpace="garden" members={[{ id: 'living_builder', name: 'Alex' }]} conversations={[{ id: 'builder-lounge', title: 'Builder lounge' }]} resources={[{ id: 'builder-notes', title: 'Builder notes', kind: 'page', space: 'builder-partition' }]} openConversation={() => { throw Error('Offline fixture only.'); }} openResource={() => { throw Error('Offline fixture only.'); }} command={tierCommand} />}
    openPage={() => setOpenedPage(true)} notifications={<SpaceJoinNotifications requests={requests} decide={async (id) => setRequests((all) => all.filter((r) => r.id !== id))} />}
    conversations={<Messenger conversations={[conversation]} identity="living_builder" name="Alex" connected={false} status="Synthetic fixture" messages={messages} selected={conversation.id} select={() => {}} footer={null} />} /></main></div></>;
}
const appearance = window.matchMedia('(prefers-color-scheme: dark)');
function applyTheme() { document.documentElement.dataset.theme = new URLSearchParams(location.search).get('theme') ?? (appearance.matches ? 'dark' : 'light'); }
applyTheme(); appearance.addEventListener('change', applyTheme);
createRoot(document.getElementById('root')!).render(<Fixture />);
