import type { Conversation, Message } from './messages.ts';

export type MessageHit = { space: string; title: string; message: Message };
export type SearchCursor = { at: number; space: string; id: string };
export type MessageSearchPage = { hits: MessageHit[]; next: SearchCursor | null };
const fold = (text: string) => text.normalize('NFC').toLocaleLowerCase('pt-BR');
function key(hit: MessageHit): SearchCursor { return { at: hit.message.at_ms, space: hit.space, id: hit.message.id }; }
function compare(a: SearchCursor, b: SearchCursor): number { return b.at - a.at || a.space.localeCompare(b.space) || a.id.localeCompare(b.id); }

/** Search this browser's decrypted history, including thread replies. Phone-only history is not searched. */
export function searchMessages(conversations: Conversation[], query: string, scope = '', after: SearchCursor | null = null, limit = 20): MessageSearchPage {
  const term = fold(query.trim());
  if (!term) return { hits: [], next: null };
  const size = Number.isFinite(limit) ? Math.max(1, Math.min(100, Math.trunc(limit))) : 20;
  const matches = conversations.filter((c) => !scope || c.id === scope).flatMap((c) => c.messages
    .filter((m) => m.entry !== null && !m.deleted && fold(m.text).includes(term))
    .map((message) => ({ space: c.id, title: c.title, message })))
    .filter((hit) => !after || compare(key(hit), after) > 0)
    .sort((a, b) => compare(key(a), key(b)));
  const hits = matches.slice(0, size);
  return { hits, next: matches.length > size ? key(hits[hits.length - 1]!) : null };
}
