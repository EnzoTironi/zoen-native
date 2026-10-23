import { useState } from 'react';
import type { Message, Messages } from './messages.ts';
import { FileView, kindOf } from './media.tsx';
import { Avatar, day, Icon, time } from './messenger-presentation.tsx';

const QUICK = ['👍', '❤️', '😂', '😮', '😢', '🙏'];
function Ticks({ state }: { state: Message['ticks'] | 'sending' | 'failed' }) {
  const label = { sending: 'Enviando', failed: 'Não enviada', sent: 'Enviada', delivered: 'Entregue', read: 'Lida' }[state ?? 'sent'];
  if (state === 'sending') return <svg className="zm-ticks" viewBox="0 0 16 11" role="img" aria-label={label}><circle cx="8" cy="5.5" r="4" fill="none" stroke="currentColor" strokeWidth="1.4" /><path d="M8 3.5v2l1.4 1" fill="none" stroke="currentColor" strokeWidth="1.4" /></svg>;
  if (state === 'failed') return <span className="zm-failed" role="img" aria-label={label}>!</span>;
  return <svg className={`zm-ticks zm-ticks-${state}`} viewBox="0 0 18 11" role="img" aria-label={label}>
    <path d="M1 6l3 3 6-7" />{state !== 'sent' ? <path d="M7 9l1 0 6-7" /> : null}
  </svg>;
}

export function Bubble({ entry, own, first, onReact, onReply, onThread, onEdit, onDelete, onForward, inThread, messages }: {
  entry: Message; messages: Messages | null; own: boolean; first: boolean; inThread?: boolean;
  onReact: (emoji: string | null) => void; onReply: () => void; onThread: () => void; onEdit: () => void; onDelete: () => void; onForward: () => void;
}) {
  const [picker, setPicker] = useState(false);
  const [emojiDraft, setEmojiDraft] = useState('');
  const [details, setDetails] = useState(false);
  const [menu, setMenu] = useState(false);
  const ordered = entry.entry !== null;
  const mine = entry.reactions.find((r) => r.mine)?.emoji ?? null;
  const state = entry.delivery === 'confirmed' ? entry.ticks : entry.delivery as 'sending' | 'failed';
  return <article className={`zm-row${own ? ' zm-own' : ''}${first ? ' zm-first' : ''}`} data-testid="message" id={`message-${entry.id}`} tabIndex={-1} onMouseLeave={() => { setPicker(false); setMenu(false); }}>
    {!own && first ? <span className="zm-author">{entry.name}</span> : null}
    <div className="zm-bubble-wrap">
      <div className={`zm-bubble${entry.deleted ? ' zm-deleted' : ''}${entry.file && !entry.deleted ? ` zm-media zm-media-${kindOf(entry.file.mime)}` : ''}`}>
        {entry.forwarded && !entry.deleted ? <span className="zm-forwarded">Encaminhada</span> : null}
        {entry.quote && !entry.deleted ? <span className="zm-quote"><b>{entry.quote.name || 'Mensagem'}</b>{entry.quote.text || 'Mensagem apagada'}</span> : null}
        {entry.file && !entry.deleted ? <FileView file={entry.file} messages={messages} presentation={entry.media?.presentation} /> : null}
        {entry.deleted || entry.text || !entry.file ? <span className="zm-text">{entry.deleted ? 'Mensagem apagada' : entry.text}</span> : null}
        <span className="zm-meta">{entry.edited && !entry.deleted ? 'editada · ' : ''}<time dateTime={new Date(entry.at_ms).toISOString()}>{time(entry.at_ms)}</time>{own ? <Ticks state={state} /> : null}</span>
      </div>
      {ordered && !entry.deleted ? <div className="zm-actions" role="toolbar" aria-label="Ações da mensagem">
        <button aria-label="Reagir" aria-expanded={picker} onClick={() => { setEmojiDraft(''); setPicker((v) => !v); }}><Icon name="smile" /></button>
        {!inThread ? <><button aria-label="Responder" onClick={onReply}><Icon name="reply" /></button>
          <button aria-label="Responder em thread" onClick={onThread}><Icon name="thread" /></button></> : null}
        <button aria-label="Mais ações" aria-expanded={menu} onClick={() => setMenu((v) => !v)}><Icon name="more" /></button>
      </div> : null}
      {picker ? <div className="zm-scrim" onClick={() => setPicker(false)} onKeyDown={(e) => { if (e.key === 'Escape') setPicker(false); }}>
        <div className="zm-sheet" role="dialog" aria-modal="true" aria-label="Escolher reação" onClick={(e) => e.stopPropagation()}>
          <h2>Escolher reação</h2>
          <div>{QUICK.map((emoji) => <button key={emoji} type="button" className="zm-pill" aria-pressed={emoji === mine} onClick={() => { setPicker(false); onReact(emoji === mine ? null : emoji); }}>{emoji}</button>)}</div>
          <form onSubmit={(e) => { e.preventDefault(); const emoji = emojiDraft.trim(); if (emoji && new TextEncoder().encode(emoji).length <= 32) { setPicker(false); onReact(emoji === mine ? null : emoji); } }}>
            <p><label>Outro emoji <input aria-label="Outro emoji" placeholder="Digite ou cole um emoji" value={emojiDraft} maxLength={64} autoFocus onChange={(e) => setEmojiDraft(e.target.value)} /></label></p>
            <p>Use o teclado de emojis do seu aparelho para escolher qualquer emoji.</p>
            {new TextEncoder().encode(emojiDraft.trim()).length > 32 ? <p role="alert">Escolha um emoji de até 32 bytes.</p> : null}
            <button className="zm-pill" type="submit" disabled={!emojiDraft.trim() || new TextEncoder().encode(emojiDraft.trim()).length > 32}>Reagir</button>
          </form>
          {mine ? <button className="zm-pill" onClick={() => { setPicker(false); onReact(null); }}>Remover minha reação</button> : null}
          <button className="zm-pill" onClick={() => setPicker(false)}>Fechar</button>
        </div>
      </div> : null}
      {menu ? <div className="zm-menu" role="menu">
        <button role="menuitem" onClick={() => { setMenu(false); onForward(); }}>Encaminhar</button>
        {entry.file ? null : <button role="menuitem" onClick={() => { setMenu(false); void navigator.clipboard?.writeText(entry.text); }}>Copiar</button>}
        {own && !entry.file ? <><button role="menuitem" onClick={() => { setMenu(false); onEdit(); }}>Editar</button>
          </> : null}
        {own ? <button role="menuitem" className="zm-danger" onClick={() => { setMenu(false); onDelete(); }}>Apagar para todos</button> : null}
      </div> : null}
    </div>
    {entry.reactions.length ? <div className="zm-reactions">{entry.reactions.map((r) => <button key={r.emoji} className={r.mine ? 'zm-on' : ''} aria-label={`Ver quem reagiu com ${r.emoji}: ${r.count}`} onClick={() => setDetails(true)}>{r.emoji}{r.count > 1 ? <span>{r.count}</span> : null}</button>)}</div> : null}
    {details ? <div className="zm-scrim" onClick={() => setDetails(false)} onKeyDown={(e) => { if (e.key === 'Escape') setDetails(false); }}>
      <div className="zm-sheet" role="dialog" aria-modal="true" aria-label="Quem reagiu" onClick={(e) => e.stopPropagation()}>
        <h2>Quem reagiu</h2>
        {entry.reactions.map((r) => <section key={r.emoji} aria-label={`${r.emoji}: ${r.count} reações`}>
          <h3>{r.emoji} {r.count}</h3>
          {r.participants.length ? <ul>{r.participants.map((participant) => <li key={participant.identity}>{participant.name || 'Alguém'}</li>)}</ul> : <p>Os nomes de quem reagiu não estão disponíveis.</p>}
          <button className="zm-pill" onClick={() => { setDetails(false); onReact(r.mine ? null : r.emoji); }}>{r.mine ? 'Remover minha reação' : `Reagir com ${r.emoji}`}</button>
        </section>)}
        {!entry.reactions.length ? <p>Nenhuma reação.</p> : null}
        <button className="zm-pill" autoFocus onClick={() => setDetails(false)}>Fechar</button>
      </div>
    </div> : null}
    {!inThread && entry.replies ? <button className="zm-thread-summary" onClick={onThread}>
      <span className="zm-stack">{entry.repliers.map((n) => <Avatar key={n} name={n} size={20} />)}</span>
      <b>{entry.replies === 1 ? '1 resposta' : `${entry.replies} respostas`}</b>{entry.last_reply_ms ? <span>{day(entry.last_reply_ms)}</span> : null}
    </button> : null}
  </article>;
}

