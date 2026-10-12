import { useEffect, useRef, useState } from 'react';
import { createRoot } from 'react-dom/client';
import QRCode from 'qrcode';
import { ShellIcon, ZoenShell, type ShellTheme } from '../src/shell.tsx';
import { message } from './boundary.ts';
import { unavailableCapability } from './capabilities.ts';
import { Pairing, type PairingState } from './pairing.ts';
import { Messages, type Conversation, type Invite, type MessagesState } from './messages.ts';
import { inviteFromPath, relayUrl, type StoredSession } from './session.ts';
import { BrowserVault } from './vault.ts';
import { CoreWorker } from './worker-client.ts';
import './web.css';

type Runtime = { vault: BrowserVault; core: CoreWorker };
type Boot = { kind: 'loading' } | { kind: 'unavailable'; message: string } |
  { kind: 'ready'; runtime: Runtime; session: StoredSession };
const invitation = inviteFromPath(location.pathname);

function Gate({ children }: { children: React.ReactNode }) {
  return <main className="zw-gate"><section className="zw-pair">{children}</section></main>;
}

function PairingGate({ runtime, session, linked }: { runtime: Runtime; session: StoredSession; linked: (session: StoredSession) => void }) {
  const [relay, setRelay] = useState(() => session.kind === 'pending' ? session.pending.relay : new URLSearchParams(location.search).get('relay') ?? '');
  const [state, setState] = useState<PairingState | { kind: 'idle' }>({ kind: 'idle' });
  const [qr, setQr] = useState('');
  const [copied, setCopied] = useState(false);
  const running = useRef<Pairing | null>(null);
  const retained = useRef(session);
  const checkFocus = useRef<HTMLDivElement>(null);
  const errorFocus = useRef<HTMLParagraphElement>(null);
  function start() {
    running.current?.stop();
    const pairing = new Pairing(runtime.vault, runtime.core, (next) => {
      if (next.kind === 'linked') { linked({ kind: 'linked', account: next.account, snapshot: '' }); return; }
      if (next.kind === 'waiting') retained.current = { kind: 'pending', pending: next.pending };
      setState(next);
    });
    running.current = pairing;
    void pairing.start(relay, retained.current);
  }
  useEffect(() => { if (session.kind === 'pending') start(); return () => running.current?.stop(); }, []);
  useEffect(() => {
    if (state.kind === 'waiting') checkFocus.current?.focus();
    if (state.kind === 'failed') errorFocus.current?.focus();
  }, [state.kind]);
  useEffect(() => {
    let disposed = false;
    if (state.kind === 'waiting') {
      void QRCode.toDataURL(state.pending.code, { width: 528, margin: 3, errorCorrectionLevel: 'M' })
        .then((url) => { if (!disposed) setQr(url); }).catch((error: unknown) => setState({ kind: 'failed', message: message(error) }));
    }
    return () => { disposed = true; };
  }, [state.kind === 'waiting' ? state.pending.code : '']);
  return <Gate>
    <h1>{invitation ? 'Open your invitation' : 'Zoen on the web'}</h1>
    <p>{invitation ? 'Link this browser from Zoen on your phone to continue into the conversation.' : 'Link this browser from Zoen on your phone. Your phone approves it as another device of your account.'}</p>
    {state.kind === 'idle' || state.kind === 'failed' ? <form onSubmit={(event) => { event.preventDefault(); start(); }}>
      <label>Relay address<input type="url" placeholder="https://your-relay.example" value={relay} required autoComplete="url" autoFocus
        onChange={(event) => setRelay(event.target.value)} onBlur={() => { try { setRelay(relayUrl(relay)); } catch { /* Submit reports invalid origins. */ } }} /></label>
      <button className="zw-primary" type="submit">Link this browser</button>
    </form> : null}
    {state.kind === 'starting' ? <p role="status">Creating this browser's protected device…</p> : null}
    {state.kind === 'waiting' ? <>
      {qr ? <img className="zw-qr" src={qr} alt="Pairing code to scan with Zoen on your phone" /> : <p>Preparing pairing code…</p>}
      <p>On your phone, open Devices, choose Link device, and scan this code.</p>
      <div className="zw-check" aria-label={`Check code ${state.pending.check}`} tabIndex={-1} ref={checkFocus}>{state.pending.check}</div>
      <p className="zw-status">Compare these six digits on both screens before your phone approves the browser.</p>
      <p className="zw-status" role="status">{state.connection.kind === 'ready' ? 'Waiting for your phone to approve…' :
        state.connection.kind === 'reconnecting' ? 'Reconnecting to the relay…' : 'Connecting securely…'}</p>
      <details><summary>Use a text code</summary><p className="zw-code">{state.pending.code}</p>
        <button className="zw-secondary" onClick={() => { void navigator.clipboard.writeText(state.pending.code).then(() => setCopied(true))
          .catch(() => setState({ kind: 'failed', message: 'The browser blocked copying. Select the text code to copy it.' })); }}>{copied ? 'Copied' : 'Copy code'}</button></details>
    </> : null}
    {state.kind === 'failed' ? <p className="zw-error" role="alert" tabIndex={-1} ref={errorFocus}>{state.message}</p> : null}
  </Gate>;
}

function Thread({ conversation, identity, connected, send }: {
  conversation: Conversation; identity: string; connected: boolean; send: (text: string) => Promise<void>;
}) {
  const [draft, setDraft] = useState('');
  const [sending, setSending] = useState(false);
  const [error, setError] = useState('');
  const end = useRef<HTMLDivElement>(null);
  const canSend = connected && conversation.can_send && !sending;
  useEffect(() => { end.current?.scrollIntoView({ block: 'end' }); }, [conversation.messages.length]);
  function submit() {
    if (!canSend || !draft.trim()) return;
    setSending(true); setError('');
    void send(draft).then(() => setDraft('')).catch((failure: unknown) => setError(message(failure))).finally(() => setSending(false));
  }
  return <section className="zw-thread" aria-label={conversation.title}>
    {conversation.waiting ? <p className="zw-status" role="status">{conversation.waiting}</p> : null}
    {conversation.unreadable ? <p className="zw-status">{conversation.unreadable} earlier encrypted {conversation.unreadable === 1 ? 'message is' : 'messages are'} available on your phone.</p> : null}
    <div className="zw-messages" role="log" aria-label="Messages" aria-live="polite">
      {conversation.messages.map((entry) => <article key={entry.id} className={`zw-bubble${entry.author === identity ? ' zw-bubble-own' : ''}`}>
        <strong>{entry.name || 'You'}</strong><div>{entry.text}</div>
        <small>{Number.isFinite(new Date(entry.at_ms).getTime()) ? <time dateTime={new Date(entry.at_ms).toISOString()}>{new Date(entry.at_ms).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}</time> : 'Time unavailable'}
          {entry.delivery === 'sending' ? ' · Sending…' : entry.delivery === 'failed' ? ` · ${entry.failure || 'Send failed'}` : ''}</small>
      </article>)}
      <div ref={end} />
    </div>
    {error ? <p className="zw-error" role="alert">{error}</p> : null}
    <form className="zw-compose" onSubmit={(event) => { event.preventDefault(); submit(); }}>
      <label className="zw-compose-label"><span className="zw-sr-only">Message to {conversation.title}</span>
        <textarea value={draft} disabled={!canSend} maxLength={32_768} placeholder={canSend ? 'Message' : conversation.waiting ?? 'Reconnecting…'}
          onChange={(event) => setDraft(event.target.value)} onKeyDown={(event) => {
            if ((event.metaKey || event.ctrlKey) && event.key === 'Enter' && !event.nativeEvent.isComposing) { event.preventDefault(); submit(); }
          }} /></label>
      <button className="zw-primary" type="submit" disabled={!canSend || !draft.trim()}>{sending ? 'Saving…' : 'Send'}</button>
    </form>
  </section>;
}

type InviteState = { kind: 'idle' } | { kind: 'loading' } | { kind: 'ready'; invite: Invite } |
  { kind: 'joining'; invite: Invite } | { kind: 'failed'; message: string };
function InviteLoader({ code, messages, changed }: {
  code: string; messages: React.RefObject<Messages | null>; changed: React.Dispatch<React.SetStateAction<InviteState>>;
}) {
  useEffect(() => {
    if (!messages.current) return;
    changed({ kind: 'loading' });
    let disposed = false;
    void messages.current.preview(code).then((invite) => { if (!disposed) changed({ kind: 'ready', invite }); })
      .catch((error: unknown) => { if (!disposed) changed({ kind: 'failed', message: message(error) }); });
    return () => { disposed = true; };
  }, [code, messages, changed]);
  return <p role="status">Opening the invitation…</p>;
}
function Invitation({ code, messages, connected, conversations, notice, opened, cancel }: {
  code: string; messages: React.RefObject<Messages | null>; connected: boolean; conversations: Conversation[];
  notice: string | null; opened: (space: string) => void; cancel: () => void;
}) {
  const [state, setState] = useState<InviteState>({ kind: 'idle' });
  useEffect(() => {
    if ((state.kind === 'ready' || state.kind === 'joining') && conversations.some((conversation) => conversation.id === state.invite.space)) opened(state.invite.space);
  }, [state, conversations]);
  return <section className="zw-account"><h1>{state.kind === 'ready' || state.kind === 'joining' ? state.invite.title : 'Your invitation'}</h1>
    {state.kind === 'idle' || state.kind === 'loading' ? connected ? <InviteLoader code={code} messages={messages} changed={setState} /> : <p role="status">Connecting securely…</p> : null}
    {state.kind === 'ready' ? <><p>{state.invite.members} {state.invite.members === 1 ? 'member' : 'members'}</p>
      <button className="zw-primary" disabled={!connected} onClick={() => {
        if (!messages.current) return;
        const invite = state.invite;
        setState({ kind: 'joining', invite });
        void messages.current.join({ invite, code }).catch((error: unknown) => setState({ kind: 'failed', message: message(error) }));
      }}>Join conversation</button></> : null}
    {state.kind === 'joining' ? <p role="status">{notice ? 'The invitation could not be accepted.' : 'Waiting for the conversation to confirm your membership…'}</p> : null}
    {notice || state.kind === 'failed' ? <p className="zw-error" role="alert">{state.kind === 'failed' ? state.message : notice}</p> : null}
    <div className="zw-pair-actions"><a className="zw-secondary" href={`zoen://join/${code}`}>Open in the native app</a>
      <button className="zw-secondary" onClick={cancel}>Back to chats</button>
      {state.kind === 'failed' || notice ? <button className="zw-secondary" disabled={!connected} onClick={() => setState({ kind: 'idle' })}>Try again</button> : null}</div>
  </section>;
}

function Linked({ runtime, session, forget }: { runtime: Runtime; session: Extract<StoredSession, { kind: 'linked' }>; forget: () => void }) {
  const [theme, setTheme] = useState<ShellTheme>('system');
  const [search, setSearch] = useState('');
  const [state, setState] = useState<MessagesState>({ kind: 'loading' });
  const [selected, setSelected] = useState('');
  const [showInvite, setShowInvite] = useState(invitation !== null);
  const running = useRef<Messages | null>(null);
  useEffect(() => {
    const messages = new Messages({ ...runtime, session, changed: setState });
    running.current = messages;
    void messages.start().catch(() => {});
    return () => { messages.stop(); running.current = null; };
  }, [runtime, session]);
  const conversations = state.kind === 'loading' ? [] : state.conversations;
  const visible = conversations.filter((conversation) => conversation.title.toLocaleLowerCase().includes(search.toLocaleLowerCase()));
  const conversation = conversations.find((entry) => entry.id === selected) ?? visible[0];
  const connected = state.kind === 'active' && state.connection.kind === 'ready';
  const status = state.kind === 'loading' ? 'Opening encrypted conversations…' : state.kind === 'failed' ? state.message :
    state.connection.kind === 'ready' ? 'Connected' : state.connection.kind === 'reconnecting' ? 'Reconnecting…' : 'Connecting securely…';
  return <ZoenShell navigation={[{ id: 'chats', label: 'Chats', icon: <ShellIcon name="chats" /> }]} activeNavigationId="chats"
    onNavigate={() => {}} chats={visible.map((entry) => ({ id: entry.id, title: entry.title, preview: entry.messages.at(-1)?.text ?? entry.waiting ?? 'Encrypted conversation' }))}
    {...(conversation ? { activeChatId: conversation.id } : {})} onSelectChat={setSelected} searchQuery={search} onSearchChange={setSearch}
    theme={theme} onThemeChange={setTheme} title={conversation?.title ?? 'Chats'} sidebarFooter={<div className="zw-footer"><span>{session.account.name}</span>
      <span className={state.kind === 'failed' ? 'zw-error' : 'zw-status'} role={state.kind === 'failed' ? 'alert' : 'status'}>{status}</span>
      {state.kind === 'failed' ? <button className="zw-secondary" onClick={() => location.reload()}>Reload Zoen</button> : null}
      <button className="zw-secondary" onClick={forget}>Forget this browser</button></div>} emptySidebar={search ? 'No matching conversations' : 'Your linked conversations'}>
    {showInvite && invitation ? <Invitation code={invitation} messages={running} connected={connected} conversations={conversations}
      notice={state.kind === 'active' ? state.notice : state.kind === 'failed' ? state.message : null}
      opened={(space) => { setSelected(space); setShowInvite(false); }} cancel={() => setShowInvite(false)} /> : conversation ? <Thread key={conversation.id} conversation={conversation} identity={session.account.identity} connected={connected}
      send={(text) => running.current?.send({ space: conversation.id, text }) ?? Promise.reject(new Error('The browser session is closed.'))} /> :
      <section className="zw-account"><h1>{state.kind === 'loading' ? 'Opening your conversations' : 'Your conversations'}</h1>
        <p>{connected ? 'Conversations you join on your phone appear here. Keep your phone connected while it adds this browser to each encrypted conversation.' : status}</p></section>}
  </ZoenShell>;
}

function App() {
  const [boot, setBoot] = useState<Boot>({ kind: 'loading' });
  useEffect(() => {
    let closed = false;
    let runtime: Runtime | undefined;
    let release = () => {};
    const held = new Promise<void>((resolve) => { release = resolve; });
    void unavailableCapability().then(async (unavailable) => {
      if (closed) return;
      if (unavailable) { setBoot({ kind: 'unavailable', message: unavailable }); return; }
      await navigator.locks.request('zoen-browser-client', { ifAvailable: true }, async (lock) => {
        if (!lock) { setBoot({ kind: 'unavailable', message: 'Zoen is already open in another tab. Continue there or close it first.' }); return; }
        const vault = await BrowserVault.open();
        let core: CoreWorker | undefined;
        try {
          const session = await vault.load();
          core = await CoreWorker.create(vault);
          runtime = { vault, core };
          if (closed) return;
          setBoot({ kind: 'ready', runtime, session });
          await held;
        } finally { core?.close(); vault.close(); }
      });
    }).catch((error: unknown) => { if (!closed) setBoot({ kind: 'unavailable', message: message(error) }); });
    return () => { closed = true; runtime?.core.close(); release(); };
  }, []);
  if (boot.kind === 'loading') return <Gate><h1>Zoen</h1><p role="status">Opening encrypted browser storage…</p></Gate>;
  if (boot.kind === 'unavailable') return <Gate><h1>Open Zoen on your phone</h1><p role="alert">{boot.message}</p></Gate>;
  if (boot.session.kind === 'linked') return <Linked runtime={boot.runtime} session={boot.session} forget={() => {
    if (!window.confirm('Forget this browser and its locally stored conversations? Unlink it from Devices on your phone as well.')) return;
    boot.runtime.core.close();
    void boot.runtime.vault.erase().then(() => location.reload()).catch((error: unknown) => setBoot({ kind: 'unavailable', message: message(error) }));
  }} />;
  return <PairingGate runtime={boot.runtime} session={boot.session} linked={(session) => setBoot({ ...boot, session })} />;
}

const root = document.getElementById('root');
if (!root) throw new Error('The Zoen root is missing.');
createRoot(root).render(<App />);
