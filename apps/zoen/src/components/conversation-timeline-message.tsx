import type { MiniappTimelineActivity } from '../core/conversation-timeline';
import { sameMessageDay } from '../core/message-presentation';
import { Text, View } from 'react-native';
import { t, formatLocale } from '../i18n';
import { useTheme } from '../theme';
import { MiniappTimelineActivityView } from '../miniapps/timeline-activity';
import { ConversationMessage, type ConversationMessageProps } from './conversation-message';

export function ConversationTimelineMessage({
  activity,
  ...message
}: ConversationMessageProps & { activity?: MiniappTimelineActivity }) {
  const { presentation: p, localDate: now } = useTheme();
  const first = activity?.entries[0] ?? message.entry;
  const at = new Date(Number(first.at_ms));
  const yesterday = new Date(now.getFullYear(), now.getMonth(), now.getDate() - 1);
  const equalDate = (date: Date) =>
    at.getFullYear() === date.getFullYear() &&
    at.getMonth() === date.getMonth() &&
    at.getDate() === date.getDate();
  const dateLabel =
    Number.isFinite(at.getTime()) && !sameMessageDay(message.previous, first)
      ? equalDate(now)
        ? t('chat.today')
        : equalDate(yesterday)
          ? t('chat.yesterday')
          : at.toLocaleDateString(formatLocale, {
              day: 'numeric',
              month: 'long',
              ...(at.getFullYear() === now.getFullYear() ? {} : { year: 'numeric' }),
            })
      : null;
  const content = activity ? (
    <MiniappTimelineActivityView activity={activity} highlighted={message.highlighted} />
  ) : (
    <ConversationMessage {...message} />
  );
  return (
    <View>
      {dateLabel ? (
        <View style={{ alignItems: 'center', paddingTop: 8, paddingBottom: 10 }}>
          <Text
            accessibilityRole="header"
            testID={`message-date-${first.id}`}
            style={{
              color: p.chat.service.text,
              backgroundColor: p.chat.service.background,
              borderRadius: 11,
              overflow: 'hidden',
              paddingHorizontal: 10,
              paddingVertical: 3,
              fontSize: 13,
              fontWeight: '500',
            }}
          >
            {dateLabel}
          </Text>
        </View>
      ) : null}
      {content}
    </View>
  );
}
