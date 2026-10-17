import { useEffect, useLayoutEffect, useRef, useState, type ReactNode } from 'react';
import type { Conversation, Messages } from './messages.ts';
import { LivingSpace, SpaceJoinNotifications, livingSpacesEnabled } from './living-space.tsx';
import { LivePage, type PageDto } from './live-page.tsx';
import { Hackathon, CommunityTiers } from './hackathon.tsx';
import { parseSpaceHome, type SpaceHome, type SpaceItem } from './space-data.ts';
import { eventPreview } from './event-preview.ts';

interface Props { conversation: Conversation; allConversations: readonly Conversation[]; controller: Messages | null; identityId: string; children: ReactNode; openConversation: (id: string) => void; selectSpace: (id: string) => void }
interface JoinNotice { id: string; name: string; spaceTitle: string; space: string; identity: string }
interface ClaimNotice { id: string; name: string; space: string; identity: string; invite: string }
/** A thin projection/controller boundary: all state, authorization and cryptography remain in the core. */
export function LivingSpaceController({ conversation, allConversations, controller, identityId, children, openConversation, selectSpace }: Props) {
  let enabled = false;
  try { enabled = livingSpacesEnabled(localStorage); } catch { /* Default off. */ }
  const [home, setHome] = useState<SpaceHome | null>(null);
  const [pages, setPages] = useState<PageDto[]>([]);
  const [selectedPage, setSelectedPage] = useState<PageDto | null>(null);
  const [pageDirty, setPageDirty] = useState(false);
  const [selectedNote, setSelectedNote] = useState<SpaceItem | null>(null);
  const [folder, setFolder] = useState(false);
  const [coverUrl, setCoverUrl] = useState<string>();
  const [failure, setFailure] = useState('');
  const [notice, setNotice] = useState('');
  const [reload, setReload] = useState(0);
  const [conversationRequest, setConversationRequest] = useState(0);
  const [joins, setJoins] = useState<JoinNotice[]>([]);
  const [claims, setClaims] = useState<ClaimNotice[]>([]);
  const [reviewBusy, setReviewBusy] = useState(false);
  const [uploadBusy, setUploadBusy] = useState(false);
  const uploadedFiles = useRef(new WeakMap<File, string>());
  const restrictedPages = useRef(new Map<string, PageDto>());
  useLayoutEffect(() => { setSelectedPage(null); setSelectedNote(null); setFolder(false); setPageDirty(false); }, [conversation.id]);
  useEffect(() => {
    let cancelled = false;
    if (!enabled || conversation.kind !== 'community' || !controller) return;
    void (async () => {
      try {
        const next = parseSpaceHome(await controller.spaceHome(conversation.id));
        const ids = [...new Set([...(next.page ? [next.page] : []), ...next.announcements.filter((a) => a.kind === 'page').map((a) => a.id), ...next.tiers.filter((t) => t.space !== null).flatMap((t) => t.resources.filter((r) => r.kind === 'page').map((r) => r.id))])];
        const results = await Promise.allSettled(ids.map((id) => controller.readPage(id)));
        const details = results.flatMap((result) => result.status === 'fulfilled' ? [result.value] : []);
        if (cancelled) return;
        setHome(next); setPages(details); setFailure(results.some((result) => result.status === 'rejected') ? 'Some pages could not be loaded. Try again.' : '');
        setSelectedPage((current) => current ? details.find((page) => page.item_id === current.item_id) ?? current : null);
      } catch { if (!cancelled) { setHome(null); setPages([]); setFailure('Could not load this Space. Check your access and try again.'); } }
    })();
    return () => { cancelled = true; };
  }, [enabled, controller, conversation.id, conversation.kind, allConversations, reload]);
  useEffect(() => {
    let cancelled = false;
    if (!enabled || !controller) return;
    void (async () => {
      const requests: JoinNotice[] = []; const tierClaims: ClaimNotice[] = [];
      for (const space of allConversations.filter((c) => c.kind === 'community')) {
        try {
          const projection = parseSpaceHome(await controller.spaceHome(space.id));
          if (!projection.canManage) continue;
          for (const identity of projection.pendingRequests) requests.push({ id: `${space.id}:${identity}`, space: space.id, identity, name: projection.members.find((p) => p.id === identity)?.name ?? 'Community member', spaceTitle: space.title });
          for (const claim of projection.tierClaims) tierClaims.push({ id: `${space.id}:${claim.invite_id}:${claim.identity_id}`, space: space.id, identity: claim.identity_id, invite: claim.invite_id, name: projection.members.find((p) => p.id === claim.identity_id)?.name ?? 'Community member' });
        } catch { /* A no-longer-authorized Space contributes no private notification data. */ }
      }
      if (!cancelled) { setJoins(requests); setClaims(tierClaims); }
    })();
    return () => { cancelled = true; };
  }, [enabled, controller, allConversations, reload]);
  useEffect(() => {
    let cancelled = false; let url: string | undefined;
    setCoverUrl(undefined);
    if (home?.coverFile && controller) void controller.readFile(home.coverFile).then((file) => {
      if (cancelled || !file.mime.startsWith('image/')) return;
      url = URL.createObjectURL(new Blob([Uint8Array.from(file.bytes).buffer], { type: file.mime })); setCoverUrl(url);
    }).catch(() => { if (!cancelled) setFailure('The cover image could not be loaded. Try again.'); });
    return () => { cancelled = true; if (url) URL.revokeObjectURL(url); };
  }, [home, controller]);
  async function mutate(command: object, space = conversation.id): Promise<unknown> {
    if (!controller) throw Error('Reload Zoen to reopen this Space.');
    const result = await controller.spaceCommand(space, command);
    setNotice(typeof result === 'object' && result !== null && 'pending' in result && result.pending === true ? 'Waiting for confirmation. Access changes appear only after approval completes.' : '');
    setReload((n) => n + 1); return result;
  }
  function openPage(id: string) {
    const page = pages.find((page) => page.item_id === id);
    if (page) { setSelectedPage(page); return; }
    const item = home?.announcements.find((item) => item.id === id);
    if (item) setSelectedNote(item);
  }
  function showConversation(id: string) { openConversation(id); setConversationRequest((n) => n + 1); }
  async function uploadFiles(files: readonly File[]): Promise<string[]> {
    if (!controller) throw Error('Reload Zoen to upload files.');
    const ids: string[] = [];
    for (const file of files) { let id = uploadedFiles.current.get(file); if (!id) { id = await controller.uploadFile(conversation.id, file); uploadedFiles.current.set(file, id); } ids.push(id); }
    setReload((n) => n + 1); return ids;
  }
  async function downloadFile(id: string) {
    if (!controller) return;
    setFailure('');
    try {
      const file = await controller.readFile(id);
      const url = URL.createObjectURL(new Blob([Uint8Array.from(file.bytes).buffer], { type: file.mime }));
      const link = document.createElement('a'); link.href = url; link.download = file.name; document.body.append(link); link.click(); link.remove();
      setTimeout(() => URL.revokeObjectURL(url), 1000); setNotice('Download started.');
    } catch { setFailure('Could not download this file. Check your access and try again.'); }
  }
  if (!enabled || conversation.kind !== 'community') return <>{children}</>;
  function closeDetail(): boolean {
    if (pageDirty && !window.confirm('Leave without saving these edits?')) return false;
    setSelectedPage(null); setSelectedNote(null); setFolder(false); setPageDirty(false); return true;
  }
  const detail = selectedPage ? <LivePage key={selectedPage.item_id} page={selectedPage} onDirtyChange={setPageDirty} refresh={async () => { if (!controller) throw Error('Reload Zoen.'); return controller.readPage(selectedPage.item_id); }} save={async (request) => { if (!controller) throw Error('Reload Zoen.'); const page = await controller.savePage(request, selectedPage.space_id); setSelectedPage(page); return page; }} />
    : selectedNote ? <article className="zl-home"><h1>{selectedNote.title}</h1><p className="zl-body">{selectedNote.text}</p></article>
    : folder ? <section className="zl-home"><h1>Files</h1>{failure && <p role="alert">{failure}</p>}<label>Add files<input type="file" multiple disabled={uploadBusy} onChange={async (event) => { const input = event.currentTarget; setUploadBusy(true); try { await uploadFiles(Array.from(input.files ?? [])); input.value = ''; } catch { setFailure('Upload is unfinished. Your encrypted plan is saved; retry when connected.'); } finally { setUploadBusy(false); } }} /></label>{home?.files.map((file) => <button type="button" key={file.id} onClick={() => void downloadFile(file.id)}>{file.title}</button>)}</section> : undefined;
  return <LivingSpace detail={detail} detailTitle={selectedPage?.title ?? selectedNote?.title ?? 'Files'} closeDetail={closeDetail} title={conversation.title} members={conversation.members} {...(coverUrl !== undefined ? { coverUrl } : {})} activeSpaceId={conversation.id}
    announcements={home?.announcements.map((a) => ({ id: a.id, title: a.title, body: a.text })) ?? []}
    pages={pages.filter((page) => page.item_id === home?.page).map((page) => ({ id: page.item_id, title: page.title }))}
    {...(home ? { availablePages: home.availablePages } : {})} choosePage={async (id) => { await mutate({ action: 'set_home', page_id: id }); }}
    {...(home ? { coverChoices: home.files.filter((file) => file.mime.startsWith('image/')) } : {})} setCover={async (id) => { await mutate({ action: 'set_cover', file_id: id }); }}
    createPage={async () => { if (!controller) throw Error('Reload Zoen.'); const page = await controller.createPage(conversation.id, 'Untitled page'); await mutate({ action: 'set_home', page_id: page.item_id }); setPages((all) => [...all.filter((p) => p.item_id !== page.item_id), page]); setSelectedPage(page); }}
    canAnnounce={home?.canManage === true} pinAnnouncement={async (id) => { await mutate({ action: 'announce', item_id: id }); }} openPage={openPage}
    events={home?.events ?? []} files={home?.files.map((file) => file.id === home.coverFile && coverUrl ? { ...file, thumbnail: coverUrl } : file) ?? []} openFolder={() => setFolder(true)}
    openItem={(id) => { if (home?.files.some((file) => file.id === id)) void downloadFile(id); else { const event = home?.events.find((event) => event.id === id); if (event) setSelectedNote({ id, title: event.title, kind: 'event', text: eventPreview(event.stateJson) }); } }}
    notice={(failure || notice || !home || (controller?.pendingUploadCount ?? 0) > 0) ? <>{failure || notice || (!home ? 'Loading this Space…' : '')}{controller && controller.pendingUploadCount > 0 && <button type="button" onClick={() => void controller.retryUploads().then(() => setReload((n) => n + 1)).catch(() => setFailure('The upload is still unfinished. Try again when connected.'))}>Retry file upload</button>}</> : undefined}
    spaces={allConversations.filter((c) => c.kind === 'community').map((c) => ({ id: c.id, title: c.title }))} selectSpace={selectSpace}
    conversations={children} conversationAvailable={home?.conversationSpace !== null && home?.conversationSpace !== undefined} conversationRequest={conversationRequest} startConversation={() => { if (home?.conversationSpace) showConversation(home.conversationSpace); }}
    {...(home?.hackathon ? { hackathonTitle: home.hackathon.title } : {})}
    hackathon={home && <Hackathon hackathon={home.hackathon} identityId={identityId} isAdmin={home.canManage} isJudge={home.canJudge} members={home.members} pendingRequests={home.teamRequests} availableFiles={home.files} command={async (command) => { await mutate(command); }} openTeamConversation={async (id) => showConversation(id)} uploadFiles={uploadFiles} openFile={downloadFile} demoSlot={<p>Demo mini-app · Coming later</p>} />}
    tiers={home && <CommunityTiers tiers={home.tiers} identityId={identityId} currentSpace={conversation.id} isAdmin={home.canManage} members={home.members} claims={home.tierClaims}
      invitations={home.invitations.map((invitation) => ({ id: invitation.id, name: `${home.tiers.find((tier) => tier.id === invitation.tier_id)?.name ?? 'Membership'} · ${new Date(invitation.expires_at_ms).toLocaleString()}${invitation.revoked ? ' · Revoked' : ''}` }))}
      resources={pages.map((page) => ({ id: page.item_id, title: page.title, kind: 'page', space: page.space_id }))} conversations={allConversations.map((c) => ({ id: c.id, title: c.title }))} openConversation={showConversation} openResource={openPage}
      createRestrictedPage={async (space, title) => { if (!controller || !home?.canManage) throw Error('Membership management is not available.'); const tier = home.tiers.find((tier) => tier.space === space); if (!tier) throw Error('This membership is not available.'); const key = JSON.stringify([identityId, space, title]); const page = restrictedPages.current.get(key) ?? await controller.createPage(space, title); restrictedPages.current.set(key, page); await mutate({ action: 'tier_bind', tier_id: tier.id, item_id: page.item_id, kind: 'page' }); restrictedPages.current.delete(key); setPages((all) => [...all.filter((p) => p.item_id !== page.item_id), page]); setSelectedPage(page); }}
      command={async (command) => { const result = await mutate(command); if (typeof result === 'string') return result; if (typeof result === 'object' && result !== null && 'invite_url' in result && typeof result.invite_url === 'string') return result.invite_url; }} />}
    notificationCount={joins.length + claims.length} notifications={<div className="zl-scroll">{failure && <p className="zl-home" role="alert">{failure}</p>}{notice && <p className="zl-home" role="status">{notice}</p>}<SpaceJoinNotifications requests={joins} decide={async (id, approve) => { const request = joins.find((r) => r.id === id); if (!request) throw Error('This request is no longer available.'); await mutate({ action: approve ? 'approve_join' : 'reject_join', identity_id: request.identity }, request.space); }} />{claims.length > 0 && <section className="zl-home"><h2>Membership requests</h2>{claims.map((claim) => <article className="zl-surface" key={claim.id}><strong>{claim.name}</strong><button type="button" disabled={reviewBusy} onClick={async () => { setReviewBusy(true); try { await mutate({ action: 'tier_claim_approve', identity_id: claim.identity, invite_id: claim.invite }, claim.space); } catch { setFailure('Could not approve this request. Try again.'); } finally { setReviewBusy(false); } }}>Approve access</button></article>)}</section>}</div>} />;
}
