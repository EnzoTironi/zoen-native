import { useState } from 'react';
import type { Conversation } from './messages.ts';
import { searchMessages, type MessageHit, type SearchCursor } from './message-search.ts';

export function MessageSearchResults({ conversations, query, open }: { conversations: Conversation[]; query: string; open: (hit: MessageHit) => void }) {
  const [scope, setScope] = useState('');
  const [cursor, setCursor] = useState<SearchCursor | null>(null);
  const [previous, setPrevious] = useState<(SearchCursor | null)[]>([]);
  const page = searchMessages(conversations, query, scope, cursor);
  return <section className="zm-search-results" aria-label="Resultados de mensagens">
    <h2>Mensagens</h2>
    <p>Busca nas mensagens disponíveis neste navegador.</p>
    <label>Onde buscar <select aria-label="Onde buscar" value={scope} onChange={(e) => { setScope(e.target.value); setCursor(null); setPrevious([]); }}>
      <option value="">Todas as conversas</option>
      {conversations.map((c) => <option key={c.id} value={c.id}>{c.title}</option>)}
    </select></label>
    <ul>{page.hits.map((hit) => <li key={`${hit.space}:${hit.message.id}`}><button onClick={() => open(hit)}>
      <b>{hit.title}</b><span>{hit.message.name}: {hit.message.text}</span>
    </button></li>)}</ul>
    {!page.hits.length ? <p>Nenhuma mensagem encontrada.</p> : null}
    <nav aria-label="Páginas de mensagens">
      {previous.length ? <button onClick={() => { setCursor(previous.at(-1)!); setPrevious(previous.slice(0, -1)); }}>Anteriores</button> : null}
      {page.next ? <button onClick={() => { setPrevious([...previous, cursor]); setCursor(page.next); }}>Mais resultados</button> : null}
    </nav>
  </section>;
}
