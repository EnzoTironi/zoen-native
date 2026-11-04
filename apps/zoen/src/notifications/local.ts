import type { Snapshot } from '../core/types';

export type LocalNote = {
  identifier: string;
  title: string;
  body: string;
  spaceId: string;
  deviceId: string;
  sequence: string;
};

function eventCount(value: string): bigint | null {
  if (!/^(0|[1-9][0-9]{0,19})$/.test(value)) return null;
  const count = BigInt(value);
  return count <= 18446744073709551615n ? count : null;
}

/** Notification content is produced from the current authenticated Rust projection only. */
export function localNotes(before: Snapshot, after: Snapshot): LocalNote[] {
  const account = after.account;
  if (
    !account?.unlocked ||
    account.setup_pending ||
    before.account?.identity_id !== account.identity_id ||
    before.account.device_id !== account.device_id ||
    !after.connection.synced
  )
    return [];
  const previous = new Map(before.conversations.map((chat) => [chat.id, chat]));
  return after.conversations.flatMap((chat) => {
    const prior = previous.get(chat.id);
    const count = eventCount(chat.event_count);
    const previousCount = eventCount(prior?.event_count ?? '0');
    if (
      chat.muted ||
      chat.archived ||
      chat.last_author?.is_me ||
      chat.unread <= (prior?.unread ?? 0) ||
      !chat.members.some((member) => member.id === account.identity_id) ||
      count === null ||
      previousCount === null ||
      count <= previousCount
    )
      return [];
    const title = chat.kind === 'Direct' ? (chat.last_author?.name ?? chat.title) : chat.title;
    const body =
      chat.kind === 'Direct' || !chat.last_author
        ? chat.last_preview
        : `${chat.last_author.name}: ${chat.last_preview}`;
    return [
      {
        identifier: `zoen-local-${account.device_id}-${chat.id}-${chat.event_count}`,
        title,
        body,
        spaceId: chat.id,
        deviceId: account.device_id,
        sequence: chat.event_count,
      },
    ];
  });
}
