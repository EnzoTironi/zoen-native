import { router } from 'expo-router';
import { Text, View } from 'react-native';
import type { MiniappTimelineActivity } from '../core/conversation-timeline';
import { messageText } from '../core/presentation';
import { t, formatLocale } from '../i18n/index.ts';
import { useTheme } from '../theme';
import { MessagePressable as Pressable } from '../components/message-interaction';
import { MessageReceipt } from '../components/message-receipt';
import { CompactMiniappCard } from './compact-card';
import { APP_CARD_METRICS } from './app-card-layout';

/** Activity is not a human message: one current native card, with the original
 * version history one tap away. Opening a card grants no miniapp permissions. */
export function MiniappTimelineActivityView({
  activity,
  highlighted = false,
}: {
  activity: MiniappTimelineActivity;
  highlighted?: boolean;
}) {
  const { colors } = useTheme();
  const entry = activity.entries.at(-1)!;
  if (!('ItemEdited' in entry.kind)) return null;
  const edit = entry.kind.ItemEdited;
  const item = !entry.deleted ? activity.item : undefined;
  const side = APP_CARD_METRICS.size;
  const time = new Date(Number(entry.at_ms)).toLocaleTimeString(formatLocale, {
    hour: '2-digit',
    minute: '2-digit',
  });
  const summary =
    activity.entries.length > 1
      ? t('miniapps.timelineUpdates', { name: entry.author.name, count: activity.entries.length })
      : `${entry.author.name} · ${messageText(entry)}`;
  return (
    <View
      testID={`miniapp-activity-${activity.entries[0]!.id}`}
      style={{
        alignSelf: entry.author.is_me ? 'flex-end' : 'flex-start',
        width: side,
        gap: 7,
        marginTop: 14,
        marginBottom: 5,
        borderRadius: 24,
        borderWidth: highlighted ? 2 : 0,
        borderColor: colors.accent,
      }}
    >
      <Text
        selectable
        style={{ color: colors.secondary, fontSize: 12, lineHeight: 17, paddingHorizontal: 8 }}
      >
        {summary}
      </Text>
      {item && activity.showCard ? <CompactMiniappCard item={item} /> : null}
      {(!item || !activity.showCard) && !entry.deleted ? (
        <Text selectable style={{ color: colors.text, fontSize: 16, paddingHorizontal: 8 }}>
          {item?.title ?? edit.title}
        </Text>
      ) : null}
      <View style={{ flexDirection: 'row', alignItems: 'center', gap: 8, paddingHorizontal: 8 }}>
        {!entry.deleted ? (
          <Pressable
            testID={`miniapp-activity-history-${entry.id}`}
            accessibilityRole="button"
            accessibilityLabel={t('miniapps.timelineHistory', { title: item?.title ?? edit.title })}
            onPress={() =>
              router.push({ pathname: '/item/[id]', params: { id: edit.item_id, history: '1' } })
            }
            style={({ pressed }) => ({
              flex: 1,
              minHeight: 44,
              justifyContent: 'center',
              opacity: pressed ? 0.5 : 1,
            })}
          >
            <Text style={{ color: colors.secondary, fontSize: 12, lineHeight: 17 }}>
              {t(item ? 'miniapps.timelineCurrentHistory' : 'miniapps.timelineVersionHistory', {
                version: item?.version ?? edit.version,
              })}
            </Text>
          </Pressable>
        ) : (
          <View style={{ flex: 1 }} />
        )}
        <Text style={{ color: colors.secondary, fontSize: 11, fontVariant: ['tabular-nums'] }}>
          {time}
        </Text>
        {entry.author.is_me ? (
          <MessageReceipt delivery={entry.delivery} ink={colors.secondary} />
        ) : null}
      </View>
    </View>
  );
}
