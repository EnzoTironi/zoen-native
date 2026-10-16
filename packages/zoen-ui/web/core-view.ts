import { array, object } from './boundary.ts';
import type { CoreOperation } from './worker-client.ts';
/** These core operations return Update, not transport messages or encrypted bytes. */
export function updatesConversations(kind: CoreOperation['kind']): boolean {
  return ['bootstrap', 'receive', 'queue_message', 'queue_post', 'queue_annotation', 'mark_read', 'queue_file', 'queue_failed_file', 'fail_outbox', 'queue_join'].includes(kind);
}
/** Reproject only the read view with the actual clock/locale. Persisted state and sealed frames stay untouched. */
export function projectCurrentConversations(updateJSON: string, viewJSON: string): string {
  const update = object(JSON.parse(updateJSON));
  array(update.conversations);
  return JSON.stringify({ ...update, conversations: array(JSON.parse(viewJSON)) });
}
