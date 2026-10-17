import { useEffect, useRef, useState } from 'react';
import { createRoot } from 'react-dom/client';
import QRCode from 'qrcode';
import { message } from './boundary.ts';
import { unavailableCapability } from './capabilities.ts';
import { Pairing, type PairingState } from './pairing.ts';
import { Devices } from './devices.tsx';
import { Messages, type Conversation, type Invite, type MessagesState } from './messages.ts';
import { inviteFromPath, relayUrl, type StoredSession } from './session.ts';
import { BrowserVault } from './vault.ts';
import { CoreWorker } from './worker-client.ts';
import './web.css';
import './messenger.css';
import { Messenger } from './messenger.tsx';
import { RecoveryHelp } from './recovery.tsx';

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
    <h1>{invitation ? 'Abra seu convite' : 'Zoen no navegador'}</h1>
    <p>{invitation ? 'Conecte este navegador pelo Zoen no seu celular para entrar direto na conversa.' : 'Conecte este navegador pelo Zoen no seu celular. Ele aprova este navegador como mais um aparelho seu.'}</p>
    {state.kind === 'idle' || state.kind === 'failed' ? <RecoveryHelp /> : null}
    {state.kind === 'idle' || state.kind === 'failed' ? <form onSubmit={(event) => { event.preventDefault(); start(); }}>
      <label>Endereço do servidor<input type="url" placeholder="https://relay.tryzoen.com" value={relay} required autoComplete="url" autoFocus
        onChange={(event) => setRelay(event.target.value)} onBlur={() => { try { setRelay(relayUrl(relay)); } catch { /* Submit reports invalid origins. */ } }} /></label>
      <button className="zw-primary glass" type="submit">Conectar este navegador</button>
    </form> : null}
    {state.kind === 'starting' ? <p role="status">Preparando este navegador com segurança…</p> : null}
    {state.kind === 'waiting' ? <>
      {qr ? <img className="zw-qr" src={qr} alt="Código para escanear com o Zoen no seu celular" /> : <p>Preparando o código…</p>}
      <p>No celular, abra Aparelhos, toque em Conectar aparelho e cole o código em texto mostrado abaixo.</p>
      <div className="zw-check" aria-label={`Código de conferência ${state.pending.check}`} tabIndex={-1} ref={checkFocus}>{state.pending.check}</div>
      <p className="zw-status">Confira se os seis números são iguais nas duas telas antes de aprovar no celular.</p>
      <p className="zw-status" role="status">{state.connection.kind === 'ready' ? 'Esperando a aprovação no celular…' :
        state.connection.kind === 'reconnecting' ? 'Reconectando…' : 'Conectando…'}</p>
      <details><summary>Usar código em texto</summary><p className="zw-code">{state.pending.code}</p>
        <button className="zw-secondary" onClick={() => { void navigator.clipboard.writeText(state.pending.code).then(() => setCopied(true))
          .catch(() => setState({ kind: 'failed', message: 'O navegador bloqueou a cópia. Selecione o código para copiar.' })); }}>{copied ? 'Copiado' : 'Copiar código'}</button></details>
      <button className="zw-secondary" onClick={() => { running.current?.stop(); setState({ kind: 'idle' }); }}>Cancelar conexão</button>
    </> : null}
    {state.kind === 'failed' ? <p className="zw-error" role="alert" tabIndex={-1} ref={errorFocus}>{state.message}</p> : null}
  </Gate>;
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
  return <p role="status">Abrindo o convite…</p>;
}
function Invitation({ code, messages, connected, conversations, notice, opened, cancel }: {
  code: string; messages: React.RefObject<Messages | null>; connected: boolean; conversations: Conversation[];
  notice: string | null; opened: (space: string) => void; cancel: () => void;
}) {
  const [state, setState] = useState<InviteState>({ kind: 'idle' });
  useEffect(() => {
    if ((state.kind === 'ready' || state.kind === 'joining') && conversations.some((conversation) => conversation.id === state.invite.space)) opened(state.invite.space);
  }, [state, conversations]);
  return <section className="zw-account"><h1>{state.kind === 'ready' || state.kind === 'joining' ? state.invite.title : 'Seu convite'}</h1>
    {state.kind === 'idle' || state.kind === 'loading' ? connected ? <InviteLoader code={code} messages={messages} changed={setState} /> : <p role="status">Conectando…</p> : null}
    {state.kind === 'ready' ? <><p>{state.invite.members} {state.invite.members === 1 ? 'pessoa' : 'pessoas'}</p>
      <button className="zw-primary glass" disabled={!connected} onClick={() => {
        if (!messages.current) return;
        const invite = state.invite;
        setState({ kind: 'joining', invite });
        void messages.current.join({ invite, code }).catch((error: unknown) => setState({ kind: 'failed', message: message(error) }));
      }}>Entrar na conversa</button></> : null}
    {state.kind === 'joining' ? <p role="status">{notice ? 'Não deu para aceitar o convite.' : 'Entrando na conversa…'}</p> : null}
    {notice || state.kind === 'failed' ? <p className="zw-error" role="alert">{state.kind === 'failed' ? state.message : notice}</p> : null}
    <div className="zw-pair-actions"><a className="zw-secondary" href={`zoen://join/${code}`}>Abrir no app</a>
      <button className="zw-secondary" onClick={cancel}>Voltar para as conversas</button>
      {state.kind === 'failed' || notice ? <button className="zw-secondary" disabled={!connected} onClick={() => setState({ kind: 'idle' })}>Tentar de novo</button> : null}</div>
  </section>;
}

function Linked({ runtime, session, forget }: { runtime: Runtime; session: Extract<StoredSession, { kind: 'linked' }>; forget: () => void }) {
  const [state, setState] = useState<MessagesState>({ kind: 'loading' });
  const [selected, setSelected] = useState('');
  const [showInvite, setShowInvite] = useState(invitation !== null);
  const [showDevices, setShowDevices] = useState(false);
  const running = useRef<Messages | null>(null);
  useEffect(() => {
    const messages = new Messages({ ...runtime, session, changed: setState });
    running.current = messages;
    void messages.start().catch(() => {});
    return () => { messages.stop(); running.current = null; };
  }, [runtime, session]);
  const conversations = state.kind === 'loading' ? [] : state.conversations;
  const connected = state.kind === 'active' && state.connection.kind === 'ready';
  const status = state.kind === 'loading' ? 'Abrindo suas conversas…' : state.kind === 'failed' ? 'Algo deu errado. Recarregue a página.' :
    state.connection.kind === 'ready' ? 'Conectado' : state.connection.kind === 'reconnecting' ? 'Reconectando…' : 'Conectando…';
  return <Messenger conversations={conversations} identity={session.account.identity} name={session.account.name} connected={connected} status={status}
    messages={running} selected={selected} select={(id) => { setSelected(id); setShowInvite(false); }}
    footer={<>{state.kind === 'failed' ? <button className="zw-secondary" onClick={() => location.reload()}>Recarregar</button> : null}
      <button className="zw-secondary" onClick={() => setShowDevices(true)}>Aparelhos</button>
      <button className="zw-secondary" onClick={forget}>Sair</button></>}>
    {showDevices ? <Devices messages={running.current} current={session.account.device} close={() => setShowDevices(false)} /> : showInvite && invitation ? <Invitation code={invitation} messages={running} connected={connected} conversations={conversations}
      notice={state.kind === 'active' ? state.notice : state.kind === 'failed' ? state.message : null}
      opened={(space) => { setSelected(space); setShowInvite(false); }} cancel={() => setShowInvite(false)} /> : undefined}
  </Messenger>;
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
        if (!lock) { setBoot({ kind: 'unavailable', message: 'O Zoen já está aberto em outra aba. Continue por lá ou feche a outra aba.' }); return; }
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
  if (boot.kind === 'loading') return <Gate><h1>Zoen</h1><p role="status">Abrindo…</p></Gate>;
  if (boot.kind === 'unavailable') return <Gate><h1>Abra o Zoen no seu celular</h1><p role="alert">{boot.message}</p></Gate>;
  if (boot.session.kind === 'linked') return <Linked runtime={boot.runtime} session={boot.session} forget={() => {
    if (!window.confirm('Sair deste navegador e apagar as conversas guardadas aqui? Remova também em Aparelhos no seu celular.')) return;
    boot.runtime.core.close();
    // Chat preferences and followed threads live in localStorage; leave nothing behind.
    localStorage.removeItem('zoen.chat-prefs'); localStorage.removeItem('zoen.thread-follow');
    void boot.runtime.vault.erase().then(() => location.reload()).catch((error: unknown) => setBoot({ kind: 'unavailable', message: message(error) }));
  }} />;
  return <PairingGate runtime={boot.runtime} session={boot.session} linked={(session) => setBoot({ ...boot, session })} />;
}

const root = document.getElementById('root');
if (!root) throw new Error('The Zoen root is missing.');
createRoot(root).render(<App />);
