import type { Command, TimelineEntry } from './types';
import { MAX_SELECTED_MESSAGES } from './message-selection.ts';
import type { MessageDeletionTarget } from './generated/MessageDeletionTarget';

export type MessageDeletionChoice = 'for_me' | 'everyone';

type MessageDeletion = Omit<Extract<Command, { op: 'delete_messages' }>, 'op'>;

/** Resolve the entire selection. Never silently delete only the eligible subset. */
export function messageDeletionTargets(
  entries: readonly TimelineEntry[],
  identity: string | undefined,
  forMe = false,
): MessageDeletionTarget[] | null {
  if (!identity || !entries.length || entries.length > MAX_SELECTED_MESSAGES) return null;
  const ids = new Set<string>();
  const targets: MessageDeletionTarget[] = [];
  for (const entry of entries) {
    if (
      (entry.deleted && (!forMe || entry.thread_replies !== 0)) ||
      (!forMe && (!entry.author?.is_me || entry.author.id !== identity)) ||
      !('Message' in entry.kind) ||
      (!entry.deleted && !entry.text_revision) ||
      ids.has(entry.id)
    )
      return null;
    ids.add(entry.id);
    targets.push({
      message_id: entry.id,
      expected_revision: entry.deleted ? entry.id : entry.text_revision!,
    });
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
  canDeleteEveryone,
  signal,
  send,
}: {
  identity: string;
  device: string;
  space: string;
  resolve(): readonly TimelineEntry[];
  subscribe(changed: () => void): () => void;
  canDeleteEveryone(): boolean;
  signal?: AbortSignal;
  confirm(
    count: number,
    everyone: boolean,
    signal: AbortSignal,
  ): Promise<MessageDeletionChoice | null>;
  send(selection: MessageDeletion): Promise<unknown>;
}): Promise<boolean> {
  const targets = messageDeletionTargets(resolve(), identity, true);
  if (!targets) return false;
  const signature = JSON.stringify(targets);
  const everyone = canDeleteEveryone() && Boolean(messageDeletionTargets(resolve(), identity));
  const controller = new AbortController();
  const abort = () => controller.abort();
  signal?.addEventListener('abort', abort, { once: true });
  if (signal?.aborted) abort();
  let retired = false;
  const current = () => {
    if (
      JSON.stringify(messageDeletionTargets(resolve(), identity, true)) !== signature ||
      everyone !== (canDeleteEveryone() && Boolean(messageDeletionTargets(resolve(), identity)))
    ) {
      retired = true;
      abort();
    }
    return !retired && !controller.signal.aborted;
  };
  const detach = subscribe(current);
  try {
    if (!current()) return false;
    const choice = await confirm(targets.length, everyone, controller.signal);
    if (
      !current() ||
      (choice !== 'for_me' && choice !== 'everyone') ||
      (choice === 'everyone' && !everyone)
    )
      return false;
    await send({
      expected_identity_id: identity,
      expected_device_id: device,
      space_id: space,
      targets,
      for_me: choice === 'for_me',
    });
    return true;
  } finally {
    retired = true;
    detach();
    signal?.removeEventListener('abort', abort);
    abort();
  }
}
