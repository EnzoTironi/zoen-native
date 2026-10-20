export type HomeActionsProps = { canOrganize?: boolean };
export type HomeCardMenuProps = {
  name: string;
  pinned: boolean;
  onToggle?: () => void;
  testID?: string;
  onMoveEarlier?: () => void;
  onMoveLater?: () => void;
  onHistory?: () => void;
  onDetails?: () => void;
  conversationPinned?: boolean;
  onToggleConversation?: () => void;
  onOrganizeConversation?: () => void;
};
