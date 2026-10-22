import { memo } from 'react';
import { renderConversationMessage } from './conversation-message-face';
import { useConversationMessage, type ConversationMessageProps } from './use-conversation-message';
export type { ConversationMessageProps } from './use-conversation-message';

/** One message face in the main chat and in threads; no alternate, reduced renderer. */
export const ConversationMessage = memo(function ConversationMessage(
  props: ConversationMessageProps,
) {
  return renderConversationMessage(useConversationMessage(props));
});
