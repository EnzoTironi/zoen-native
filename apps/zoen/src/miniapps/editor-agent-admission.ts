import type { MemberAdditionState } from '../communities/member-addition';

export type EditorAgentAdmission =
  | 'loading'
  | 'failed'
  | 'ready'
  | 'waiting'
  | 'missing'
  | 'unavailable';
/** Presentation only. Both inclusion and request publication recheck Core authority. */
export function editorAgentAdmission(state: MemberAdditionState): EditorAgentAdmission {
  if (state.phase === 'failed') return 'failed';
  if (state.phase !== 'ready') return 'loading';
  const owned = state.allResults.filter((c) => c.owned_agent);
  const writers = owned.filter(
    (c) => c.role === 'owner' || c.role === 'admin' || c.role === 'member',
  );
  if (writers.some((c) => c.delivery_ready)) return 'ready';
  if (writers.length || owned.some((c) => c.state === 'Updating')) return 'waiting';
  return (!owned.length && state.canManage) || owned.some((c) => c.can_add)
    ? 'missing'
    : 'unavailable';
}
