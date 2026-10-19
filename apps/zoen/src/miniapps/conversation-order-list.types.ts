import type { ItemDetail } from '../core/generated/ItemDetail';

export type ConversationOrderListProps = {
  items: readonly ItemDetail[];
  pinnedIds: readonly string[];
  onReorder: (ids: readonly string[]) => Promise<void>;
  onSetPinned: (id: string, pinned: boolean) => Promise<void>;
};
