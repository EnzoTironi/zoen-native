// Local visual fixture of production components. No vault, relay, native engine or E2E claims.
import { useRef, useState } from 'react';
import { createRoot } from 'react-dom/client';
import { Messenger } from '../messenger.tsx';
import type { Conversation, Message, Messages } from '../messages.ts';
import '../web.css';
import '../messenger.css';
const identity = '11'.repeat(32);
const message = (id: string, text: string, author = identity): Message => ({ id, entry: id, text, author, name: author === identity ? 'Ana' : 'Bruno', at_ms: 1_792_000_000_000, delivery: 'confirmed', failure: null, ticks: 'read', edited: false, deleted: false, forwarded: false, reactions: [], quote: null, thread: null, replies: 0, repliers: [], last_reply_ms: null });
function Fixture() {
  const [fail, setFail] = useState(true);
  const [selected, select] = useState('amigos');
  const [conversations, setConversations] = useState<Conversation[]>([{ id: 'amigos', title: 'Amigos', kind: 'group', can_send: true, waiting: null, unreadable: 0, unread: 0, mentions: 0, members: 2, messages: [message('a', 'A pousada fica perto da praia.', '22'.repeat(32)), message('b', 'Vou confirmar a reserva amanhã.')] }]);
  const messages = useRef<Messages | null>(null);
  messages.current = {
    annotate: async (space: string, target: string, note: 'Delete' | { Edit: { text: string } }) => {
      if (fail) throw new Error('Não deu para salvar agora. Seu texto continua aqui. Tente de novo.');
      setConversations((all) => all.map((c) => c.id !== space ? c : { ...c, messages: c.messages.map((m) => m.entry !== target ? m : note === 'Delete' ? { ...m, text: '', deleted: true } : { ...m, text: note.Edit.text, edited: true }) }));
    },
    post: async ({ space, text }: { space: string; text: string }) => setConversations((all) => all.map((c) => c.id !== space ? c : { ...c, messages: [...c.messages, message(`local-${c.messages.length}`, text)] })),
  } as unknown as Messages;
  return <><div style={{ padding: 12, background: '#fff3c4', fontSize: 14 }}>Demonstração local — dados de teste, sem conexão. Não é um teste de ponta a ponta. <label><input type="checkbox" checked={fail} onChange={(e) => setFail(e.target.checked)} /> Simular falha ao salvar</label></div><Messenger conversations={conversations} identity={identity} name="Ana" connected status="Dados de teste" messages={messages} selected={selected} select={select} footer={null} /></>;
}
createRoot(document.getElementById('root')!).render(<Fixture />);
