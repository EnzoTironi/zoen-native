import type { HomeOwner, HomePreferencesPort } from '../home/preferences';
import {
  ConversationCardsController,
  ConversationCardsFailure,
} from '../miniapps/conversation-preferences.ts';
import type { ItemDetail } from './generated/ItemDetail';

export const ownerA: HomeOwner = { mode: 'account', identityId: 'alice', deviceId: 'phone_a' };
export const ownerB: HomeOwner = { mode: 'account', identityId: 'bruno', deviceId: 'phone_b' };
export const failure = (kind: ConversationCardsFailure['kind']) => (error: unknown) =>
  error instanceof ConversationCardsFailure && error.kind === kind;
export function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}
export function miniapp(id: string, space = 'shared'): ItemDetail {
  return {
    id,
    space_id: space,
    title: `Canonical private title ${id}`,
    app: { app_id: 'poll', snapshot_json: '{"private":"canonical only"}' },
  } as ItemDetail;
}
export function fixture() {
  let owner: HomeOwner | null = ownerA;
  let spaces = new Set(['shared', 'second']);
  let items = [miniapp('it_one'), miniapp('it_two'), miniapp('it_other', 'second')];
  const saved = new Map<string, string>();
  const port: HomePreferencesPort = {
    get: async (key) => saved.get(key) ?? null,
    set: async (key, value) => {
      saved.set(key, value);
    },
  };
  const source = { owner: () => owner, spaces: () => spaces, items: () => items };
  const cards = new ConversationCardsController(source, port);
  return {
    cards,
    source,
    port,
    saved,
    owner: (value: HomeOwner | null) => {
      owner = value;
    },
    spaces: (value: string[]) => {
      spaces = new Set(value);
    },
    items: (value: ItemDetail[]) => {
      items = value;
    },
  };
}
export const ids = (items: readonly ItemDetail[]) => items.map((item) => item.id);
export const preference = (hiddenIds: string[] = [], orderedIds: string[] = []) => ({
  hiddenIds,
  orderedIds,
});
