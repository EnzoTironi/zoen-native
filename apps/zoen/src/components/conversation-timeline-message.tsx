import type { MiniappTimelineActivity } from '../core/conversation-timeline';
import { MiniappTimelineActivityView } from '../miniapps/timeline-activity';
import { ConversationMessage, type ConversationMessageProps } from './conversation-message';

export function ConversationTimelineMessage({
  activity,
  ...message
}: ConversationMessageProps & { activity?: MiniappTimelineActivity }) {
  return activity ? (
    <MiniappTimelineActivityView activity={activity} highlighted={message.highlighted} />
  ) : (
    <ConversationMessage {...message} />
  );
}
