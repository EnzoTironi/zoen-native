import type { InboxFolder } from '../core/generated/InboxFolder';
import type { InboxFolders } from '../core/generated/InboxFolders';
import type { SpaceSummary } from '../core/generated/SpaceSummary';
import { t } from '../i18n/index.ts';

export const builtinFolders = () => [
  { id: 'all', label: t('chat.all') },
  { id: 'unread', label: t('chat.unread') },
  { id: 'Direct', label: t('chat.people') },
  { id: 'Group', label: t('chat.groups') },
  { id: 'Community', label: t('chat.communities') },
  { id: 'archived', label: t('chat.archived') },
];
export const folderTabs = (configuration?: InboxFolders) => [
  ...builtinFolders(),
  ...(configuration?.folders ?? []).map((folder) => ({ id: folder.id, label: folder.name })),
];
export function matchesFolder(
  chat: SpaceSummary,
  id: string,
  configuration?: InboxFolders,
): boolean {
  const folder = configuration?.folders.find((item) => item.id === id);
  if (!folder) {
    if (id === 'archived') return chat.archived;
    return (
      !chat.archived &&
      (id === 'all' ||
        (id === 'unread' ? chat.unread > 0 || !!chat.manual_unread : chat.kind === id))
    );
  }
  if (folder.exclude.includes(chat.id)) return false;
  // Explicitly included chats override automatic category/read/mute/archive filters, like Telegram.
  if (folder.include.includes(chat.id)) return true;
  if (folder.exclude_archived && chat.archived) return false;
  if (folder.exclude_read && chat.unread === 0 && !chat.manual_unread) return false;
  if (folder.exclude_muted && chat.muted) return false;
  return folder.kinds.some((kind) => chat.kind === kind);
}
export function newFolder(id: string): InboxFolder {
  return {
    id: `folder:${id}`,
    name: '',
    kinds: [],
    include: [],
    exclude: [],
    exclude_read: false,
    exclude_muted: false,
    exclude_archived: true,
  };
}
export function withoutFolder(configuration: InboxFolders, id: string): InboxFolders {
  return {
    folders: configuration.folders.filter((folder) => folder.id !== id),
    default_folder: configuration.default_folder === id ? 'all' : configuration.default_folder,
  };
}

export const cloneFolders = (configuration: InboxFolders): InboxFolders => ({
  default_folder: configuration.default_folder,
  folders: configuration.folders.map((folder) => ({
    ...folder,
    kinds: [...folder.kinds],
    include: [...folder.include],
    exclude: [...folder.exclude],
  })),
});
