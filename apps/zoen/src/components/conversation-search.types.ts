import type { ConversationSearchState } from '../core/conversation-search';

export type ConversationSearchProps = {
  state: ConversationSearchState;
  onChange: (query: string) => void;
  onSearch: () => void;
  onAsk: () => void;
};
export type ChatSearchControls = ConversationSearchProps & {
  onOpen: () => void;
  onClose: () => void;
  onMove: (direction: 'older' | 'newer') => void;
};
