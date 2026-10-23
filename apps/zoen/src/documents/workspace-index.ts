import type { ItemDetail } from '../core/types';

export const workspaceKinds = [
  'all',
  'pages',
  'slides',
  'sites',
  'images',
  'files',
  'apps',
] as const;
export type WorkspaceKind = (typeof workspaceKinds)[number];
export type WorkspaceFilter = {
  spaces: ReadonlySet<string>;
  space: string | null;
  kind: WorkspaceKind;
  query: string;
  folder: string;
};
export type WorkspaceRow =
  | { type: 'folder'; key: string; name: string; path: string; count: number }
  | { type: 'item'; key: string; item: ItemDetail };

export function workspaceKind(item: ItemDetail): Exclude<WorkspaceKind, 'all'> {
  if (item.kind_id === 'page') return 'pages';
  if (item.app?.app_id === 'deckcraft') return 'slides';
  if (item.app) return 'apps';
  if (item.kind_id === 'file' && item.file?.mime === 'text/html') return 'sites';
  if (item.file?.mime.startsWith('image/')) return 'images';
  return 'files';
}

const searchable = (value: string) => value.normalize('NFKD').replace(/\p{M}/gu, '').toLowerCase();

/** Paths are canonical document paths, not local filesystem locations. */
export function workspaceFolder(item: ItemDetail): string {
  const segments = item.path.split('/');
  if (segments.some((part) => !part || part === '.' || part === '..' || part.includes('\\')))
    return '';
  return segments.slice(0, -1).join('/');
}

function updated(item: ItemDetail): bigint {
  return item.versions.reduce((latest, version) => {
    try {
      const value = BigInt(version.at_ms);
      return value > latest ? value : latest;
    } catch {
      return latest;
    }
  }, 0n);
}

/** Always intersect cached results with current membership, including while a refresh fails. */
export function workspaceRows(
  items: readonly ItemDetail[],
  filter: WorkspaceFilter,
): WorkspaceRow[] {
  const words = searchable(filter.query.trim()).split(/\s+/u).filter(Boolean);
  const selected = items.filter((item) => {
    if (!filter.spaces.has(item.space_id) || (filter.space && item.space_id !== filter.space))
      return false;
    if (filter.kind !== 'all' && workspaceKind(item) !== filter.kind) return false;
    const parent = workspaceFolder(item);
    if (
      filter.space &&
      filter.folder &&
      parent !== filter.folder &&
      !parent.startsWith(filter.folder + '/')
    )
      return false;
    const text = searchable([item.title, item.path, item.text ?? '', item.space_title].join('\n'));
    return words.every((word) => text.includes(word));
  });
  // An all-chat view and searches are flat: results remain visible in nested folders.
  if (!filter.space || words.length) return recent(selected);
  const folders = new Map<string, Extract<WorkspaceRow, { type: 'folder' }>>();
  const children: ItemDetail[] = [];
  for (const item of selected) {
    const parent = workspaceFolder(item);
    if (parent === filter.folder) children.push(item);
    else {
      const relative = filter.folder ? parent.slice(filter.folder.length + 1) : parent;
      const name = relative.split('/')[0]!;
      const path = filter.folder ? `${filter.folder}/${name}` : name;
      const folder = folders.get(path) ?? {
        type: 'folder',
        key: `folder:${path}`,
        name,
        path,
        count: 0,
      };
      folder.count++;
      folders.set(path, folder);
    }
  }
  const folderRows = [...folders.values()].sort((a, b) =>
    a.path < b.path ? -1 : a.path > b.path ? 1 : 0,
  );
  return [...folderRows, ...recent(children)];
}

function recent(items: readonly ItemDetail[]): WorkspaceRow[] {
  return items
    .map((item) => ({ item, at: updated(item) }))
    .sort((a, b) =>
      a.at > b.at
        ? -1
        : a.at < b.at
          ? 1
          : a.item.id < b.item.id
            ? -1
            : a.item.id > b.item.id
              ? 1
              : 0,
    )
    .map(({ item }) => ({ type: 'item', key: `item:${item.id}`, item }));
}
