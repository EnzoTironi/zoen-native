import {
  HomeController,
  HomeFailure,
  projectHomeItems,
  type HomeOwner,
  type HomePreferencesPort,
} from '../home/preferences.ts';
import type { ItemDetail } from './generated/ItemDetail';

export const ownerA: HomeOwner = { mode: 'account', identityId: 'alice', deviceId: 'phone_a' };
export const stored = (
  pinnedIds: readonly string[],
  orderedIds: readonly string[] = [],
  hiddenIds: readonly string[] = [],
) => JSON.stringify({ version: 3, pinnedIds, orderedIds, hiddenIds });
export const failure = (kind: HomeFailure['kind']) => (error: unknown) =>
  error instanceof HomeFailure && error.kind === kind;
export const ownerB: HomeOwner = { mode: 'account', identityId: 'bruno', deviceId: 'phone_b' };
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
    title: `User title ${id}`,
    app: { app_id: 'poll', snapshot_json: '{"private":"canonical only"}' },
  } as ItemDetail;
}
export function fixture() {
  let owner: HomeOwner | null = ownerA;
  let spaces = new Set(['shared']);
  let items = [miniapp('it_one'), miniapp('it_two')];
  const saved = new Map<string, string>();
  const port: HomePreferencesPort = {
    get: async (key) => saved.get(key) ?? null,
    set: async (key, value) => {
      saved.set(key, value);
    },
  };
  const source = {
    owner: () => owner,
    spaces: () => spaces,
    items: async () => items,
  };
  const home = new HomeController(source, port);
  return {
    home,
    port,
    source,
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

export const projection = (home: HomeController) => {
  const state = home.getSnapshot();
  const { items, hiddenItems, allItems } = projectHomeItems(state.items, state);
  return {
    visible: items.map((item) => item.id),
    hidden: hiddenItems.map((item) => item.id),
    all: allItems.map((item) => item.id),
  };
};
