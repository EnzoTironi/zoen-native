import type { ItemDiscussionDto } from '../core/generated/ItemDiscussionDto';
import { homeSpacesFor } from '../home/access.ts';
import { documentScope as discussionScope } from './document-authority.ts';
export { documentScope as discussionScope } from './document-authority.ts';
type Session = Parameters<typeof homeSpacesFor>[0];

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
