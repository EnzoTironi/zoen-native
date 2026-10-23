import type { ItemDiscussionDto } from '../core/generated/ItemDiscussionDto';
import { homeOwnerFor, homeSpacesFor } from '../home/access.ts';

type Session = Parameters<typeof homeOwnerFor>[0];
export function discussionScope(state: Session, epoch: number): string | null {
  const owner = homeOwnerFor(state);
  return owner ? JSON.stringify([owner.mode, owner.identityId, owner.deviceId, epoch]) : null;
}

/** Recheck current authority before navigating with a previously loaded root. */
export function discussionTarget(
  state: Session,
  epoch: number,
  expectedScope: string,
  spaceId: string,
  itemId: string,
  discussion: ItemDiscussionDto,
): { space: string; root: string } | null {
  if (
    discussionScope(state, epoch) !== expectedScope ||
    !homeSpacesFor(state).has(spaceId) ||
    discussion.space_id !== spaceId ||
    discussion.item_id !== itemId ||
    !discussion.root?.id ||
    discussion.root.in_thread !== null
  )
    return null;
  return { space: spaceId, root: discussion.root.id };
}
