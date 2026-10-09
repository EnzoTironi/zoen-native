import { ShellIcon, type ShellChatItem, type ShellNavigationItem } from '@zoen/ui/shell';

export type Destination =
  | 'chats'
  | 'activity'
  | 'store'
  | 'files'
  | 'agents'
  | 'context'
  | 'search';
export type ConversationKind = 'direct' | 'group' | 'community';
export type InboxFilter = ConversationKind | 'all';

export interface PreviewChat extends ShellChatItem {
  readonly kind: ConversationKind;
}

export const conversationLabels: Record<ConversationKind, string> = {
  direct: 'Direct',
  group: 'Group',
  community: 'Community',
};

export const sampleChats: readonly PreviewChat[] = [
  {
    id: 'studio',
    kind: 'group',
    title: 'Design studio',
    preview: 'A little more room to think.',
    avatar: 'DS',
    time: '10:42',
    unreadCount: 3,
  },
  {
    id: 'makers',
    kind: 'community',
    title: 'Makers community',
    preview: 'Share a prototype or ask a question.',
    avatar: 'MC',
    time: '10:38',
    unreadCount: 1,
  },
  {
    id: 'marina',
    kind: 'direct',
    title: 'Marina',
    preview: 'The new frame feels calmer.',
    avatar: 'M',
    time: '10:31',
    unreadCount: 2,
  },
  {
    id: 'saturday',
    kind: 'group',
    title: 'Saturday crew',
    preview: 'Same place, a little earlier?',
    avatar: 'SC',
    time: '09:18',
    unreadCount: 2,
  },
  {
    id: 'zoen',
    kind: 'direct',
    title: 'Zoen',
    preview: 'Your place for the things that matter.',
    avatar: 'z',
    time: 'Yesterday',
  },
  {
    id: 'research',
    kind: 'group',
    title: 'Research notes',
    preview: 'A few references for later.',
    avatar: 'RN',
    time: 'Yesterday',
  },
  {
    id: 'family',
    kind: 'group',
    title: 'Family',
    preview: 'See you this weekend.',
    avatar: 'F',
    time: 'Tue',
  },
];

export const navigation: readonly ShellNavigationItem<Destination>[] = [
  {
    id: 'chats',
    label: 'Chats',
    icon: <ShellIcon name="chats" />,
    badge: sampleChats.reduce((count, chat) => count + (chat.unreadCount ?? 0), 0),
  },
  { id: 'activity', label: 'Activity', icon: <ShellIcon name="activity" /> },
  { id: 'store', label: 'Store', icon: <ShellIcon name="store" /> },
  { id: 'files', label: 'Files', icon: <ShellIcon name="files" /> },
  { id: 'agents', label: 'Your agents', icon: <ShellIcon name="agents" /> },
  { id: 'context', label: 'Your context', icon: <ShellIcon name="context" /> },
  { id: 'search', label: 'Search', icon: <ShellIcon name="search" /> },
];

export function filterInbox(
  chats: readonly PreviewChat[],
  filter: InboxFilter,
  query: string,
): readonly PreviewChat[] {
  const normalized = query.trim().toLocaleLowerCase();
  return chats.filter(
    (chat) =>
      (filter === 'all' || chat.kind === filter) &&
      `${chat.title} ${chat.preview ?? ''} ${conversationLabels[chat.kind]}`
        .toLocaleLowerCase()
        .includes(normalized),
  );
}

export function InboxFilters({
  selected,
  onChange,
}: {
  selected: InboxFilter;
  onChange: (filter: InboxFilter) => void;
}) {
  const filters: readonly { id: InboxFilter; label: string }[] = [
    { id: 'all', label: 'All' },
    { id: 'direct', label: 'Direct' },
    { id: 'group', label: 'Groups' },
    { id: 'community', label: 'Communities' },
  ];
  return (
    <div className="preview-inbox-filters" role="group" aria-label="Filter chats">
      {filters.map((filter) => (
        <button
          key={filter.id}
          type="button"
          aria-pressed={filter.id === selected}
          onClick={() => onChange(filter.id)}
        >
          {filter.label}
        </button>
      ))}
    </div>
  );
}

export function CommunityResources() {
  return (
    <details className="preview-community-resources">
      <summary>Community resources · Sample</summary>
      <div>
        <h2>Makers community</h2>
        <p>Sample resources and roles, kept inside this conversation.</p>
        <details>
          <summary>Community guide.txt</summary>
          <pre>
            {
              "Community guide\n\nShare work in progress.\nGive useful feedback.\nAsk before sharing someone else's work."
            }
          </pre>
        </details>
        <h3>Sample member roles</h3>
        <dl>
          <dt>Moderator</dt>
          <dd>Manage invitations and pinned resources.</dd>
          <dt>Member</dt>
          <dd>Read, post messages, and share files.</dd>
        </dl>
        <p>These roles are examples. This preview does not change anyone's permissions.</p>
      </div>
    </details>
  );
}
