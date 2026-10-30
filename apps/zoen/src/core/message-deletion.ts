import type { Command, TimelineEntry } from './types';
import { MAX_SELECTED_MESSAGES } from './message-selection.ts';
import type { MessageDeletionTarget } from './generated/MessageDeletionTarget';

type MessageDeletion = Omit<Extract<Command, { op: 'delete_messages' }>, 'op'>;

/** Resolve the entire selection. Never silently delete only the eligible subset. */
export function messageDeletionTargets(
  entries: readonly TimelineEntry[],
  identity: string | undefined,
): MessageDeletionTarget[] | null {
  if (!identity || !entries.length || entries.length > MAX_SELECTED_MESSAGES) return null;
  const ids = new Set<string>();
  const targets: MessageDeletionTarget[] = [];
  for (const entry of entries) {
    if (
      entry.deleted ||
      !entry.author?.is_me ||
      entry.author.id !== identity ||
      !('Message' in entry.kind) ||
      !entry.text_revision ||
      ids.has(entry.id)
    )
      return null;
    ids.add(entry.id);
    targets.push({ message_id: entry.id, expected_revision: entry.text_revision });
  }
  return targets;
}

/** A native confirmation is bound to an exact set, session and observed revisions.
 * The Core's signed tombstones make retries idempotent, including after a lost ACK. */
export async function confirmMessageDeletion({
  identity,
  device,
  space,
  resolve,
  subscribe,
  confirm,
  send,
}: {
  identity: string;
  device: string;
  space: string;
  resolve(): readonly TimelineEntry[];
  subscribe(changed: () => void): () => void;
  confirm(count: number): Promise<boolean>;
  send(selection: MessageDeletion): Promise<unknown>;
}): Promise<boolean> {
  const targets = messageDeletionTargets(resolve(), identity);
  if (!targets) return false;
  const signature = JSON.stringify(targets);
  let retired = false;
  const current = () => {
    if (JSON.stringify(messageDeletionTargets(resolve(), identity)) !== signature) retired = true;
    return !retired;
  };
  const detach = subscribe(current);
  try {
    if (!current() || !(await confirm(targets.length)) || !current()) return false;
    await send({
      expected_identity_id: identity,
      expected_device_id: device,
      space_id: space,
      targets,
    });
    return true;
  } finally {
    retired = true;
    detach();
  }
}
