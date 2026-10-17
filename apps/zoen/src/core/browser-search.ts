import {
  searchMessages,
  type MessageHit,
  type SearchCursor,
} from '../../../../packages/zoen-ui/web/message-search.ts';
import type { Conversation } from '../../../../packages/zoen-ui/web/messages.ts';
import type { UniversalHit } from './generated/UniversalHit';
import type { Persona } from './generated/Persona';

const fold = (text: string) => text.trim().normalize('NFC').toLocaleLowerCase('pt-BR');
type NameFor = (id: string, name: string) => Persona;

function excerpt(text: string, query: string): string {
  const normalized = text.normalize('NFC');
  const index = fold(normalized).indexOf(fold(query));
  const start = Math.max(0, index - 70);
  return `${start ? '…' : ''}${normalized.slice(start, start + 220)}${normalized.length > start + 220 ? '…' : ''}`;
}

function messageHit(hit: MessageHit, query: string, persona: NameFor): UniversalHit {
  return {
    kind: 'message',
    ref_id: hit.message.id,
    space_id: hit.space,
    space_title: hit.title,
    title: hit.message.name,
    title_snippet: hit.message.name,
    snippet: excerpt(hit.message.text, query),
    at_ms: String(hit.message.at_ms),
    persona: persona(hit.message.author, hit.message.name),
  };
}

function cursor(value: string | null, query: string, space: string): SearchCursor | null {
  if (value === null) return null;
  try {
    if (value.length > 2048) throw new Error();
    const data: unknown = JSON.parse(value);
    if (
      !data ||
      typeof data !== 'object' ||
      !('query' in data) ||
      !('space' in data) ||
      !('after' in data)
    )
      throw new Error();
    if (data.query !== fold(query) || data.space !== space) throw new Error();
    const after = data.after;
    if (
      !after ||
      typeof after !== 'object' ||
      !('at' in after) ||
      !('space' in after) ||
      !('id' in after)
    )
      throw new Error();
    if (
      typeof after.at !== 'number' ||
      !Number.isSafeInteger(after.at) ||
      after.space !== space ||
      typeof after.id !== 'string'
    )
      throw new Error();
    return { at: after.at, space, id: after.id };
  } catch {
    throw new Error('Esta busca mudou. Reinicie a busca para continuar.');
  }
}

export function searchBrowserMessages(
  conversations: Conversation[],
  query: string,
  space: string,
  after: string | null,
  limit: number,
  persona: NameFor,
) {
  const page = searchMessages(conversations, query, space, cursor(after, query, space), limit);
  return {
    hits: page.hits.map((hit) => messageHit(hit, query, persona)),
    next_cursor: page.next ? JSON.stringify({ query: fold(query), space, after: page.next }) : null,
  };
}

export function searchBrowser(
  conversations: Conversation[],
  query: string,
  kinds: string[],
  limit: number,
  persona: NameFor,
) {
  const started = performance.now();
  const term = fold(query);
  const size = Number.isFinite(limit) ? Math.max(1, Math.min(100, Math.trunc(limit))) : 30;
  const allowed = (kind: string) => !kinds.length || kinds.includes(kind);
  const hits: UniversalHit[] = [];
  if (term) {
    if (allowed('space'))
      for (const chat of conversations) {
        if (!fold(chat.title).includes(term)) continue;
        hits.push({
          kind: 'space',
          ref_id: chat.id,
          space_id: chat.id,
          space_title: chat.title,
          title: chat.title,
          title_snippet: chat.title,
          snippet: '',
          at_ms: '0',
          persona: null,
        });
      }
    const seen = new Set<string>();
    for (const chat of conversations)
      for (const profile of chat.profiles) {
        if (seen.has(profile.id)) continue;
        seen.add(profile.id);
        const kind = profile.kind === 'Agent' ? 'agent' : 'person';
        if (!allowed(kind) || !fold(`${profile.name} ${profile.handle}`).includes(term)) continue;
        hits.push({
          kind,
          ref_id: profile.id,
          space_id: null,
          space_title: null,
          title: profile.name || `@${profile.handle}`,
          title_snippet: profile.name || `@${profile.handle}`,
          snippet: profile.bio,
          at_ms: '0',
          persona: persona(profile.id, profile.name),
        });
      }
    if (allowed('message'))
      hits.push(
        ...searchMessages(conversations, query, '', null, size).hits.map((hit) =>
          messageHit(hit, query, persona),
        ),
      );
  }
  return {
    hits: hits.slice(0, size),
    took_us: String(Math.max(0, Math.round((performance.now() - started) * 1000))),
  };
}
