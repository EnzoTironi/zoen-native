import { useCallback, useLayoutEffect, useRef, useState } from 'react';
import { Image } from 'expo-image';
import { router } from 'expo-router';
import { Text, View, useWindowDimensions, type TextLayoutEvent } from 'react-native';
import type { MiniappTimelineActivity } from '../core/conversation-timeline';
import { messageText } from '../core/presentation';
import { t } from '../i18n/index.ts';
import { useTheme } from '../theme';
import { MessagePressable as Pressable } from '../components/message-interaction';
import { messageDeliveryLabel } from '../components/message-receipt';
import {
  telegramServiceLineRects,
  telegramServiceMask,
  type ServiceLine,
} from './telegram-service-message';

/** Typed App changes use Telegram's centered action-message presentation.
 * A share remains a normal message with a canonical App attachment. Neither
 * presentation manufactures an event, grants permission, nor mutates an App. */
export function MiniappTimelineActivityView({
  activity,
  highlighted = false,
  disabled = false,
}: {
  activity: MiniappTimelineActivity;
  highlighted?: boolean;
  disabled?: boolean;
}) {
  const { colors, presentation: p } = useTheme();
  const { width, fontScale } = useWindowDimensions();
  const entry = activity.entries.at(-1);
  const edit = entry && 'ItemEdited' in entry.kind ? entry.kind.ItemEdited : null;
  const deleted = !entry || entry.deleted;
  const note = entry ? messageText(entry) : '';
  const attribution =
    !deleted && entry
      ? activity.entries.length > 1
        ? t('miniapps.timelineUpdates', { name: entry.author.name, count: activity.entries.length })
        : entry.author.name
      : '';
  const title = !deleted && edit && !note.includes(edit.title) ? edit.title : '';
  const delivery =
    !deleted && entry && ['Local', 'Sending', 'Failed'].includes(entry.delivery)
      ? messageDeliveryLabel(entry.delivery)
      : '';
  const label = [attribution, note, title, delivery].filter(Boolean).join(' · ');
  const owner = `${entry?.id}:${label}:${width}:${fontScale}`;
  const renderedOwner = useRef<string | null>(owner);
  const [measured, setMeasured] = useState<{
    owner: string;
    lines: readonly ServiceLine[];
    width: number;
  } | null>(null);
  useLayoutEffect(() => {
    renderedOwner.current = owner;
    return () => {
      renderedOwner.current = null;
    };
  }, [owner]);
  const onTextLayout = useCallback(
    (event: TextLayoutEvent) => {
      if (renderedOwner.current !== owner) return;
      const lines = event.nativeEvent.lines;
      const labelWidth = Math.max(...lines.map((line) => line.x * 2 + line.width));
      const rects = telegramServiceLineRects(lines, labelWidth);
      if (!rects.length) return;
      setMeasured((previous) =>
        previous?.owner === owner &&
        previous.width === labelWidth &&
        JSON.stringify(previous.lines) === JSON.stringify(rects)
          ? previous
          : { owner, lines: rects, width: labelWidth },
      );
    },
    [owner],
  );
  const mask =
    measured?.owner === owner
      ? telegramServiceMask(measured.lines, p.chat.service.background)
      : null;
  if (!entry || !edit) return null;
  // A missing/removed/foreign projection is not permission to expose its history.
  // The route performs the authoritative check again when opened.
  const canOpen = !deleted && !disabled && activity.item?.id === edit.item_id;
  const faceStyle = {
    maxWidth: '100%' as const,
    paddingTop: 1,
    paddingBottom: 3,
    borderRadius: 11,
    borderWidth: highlighted ? 2 : 0,
    borderColor: colors.accent,
    transform: [{ translateX: -1 }],
  };
  const content = (
    <>
      {mask ? (
        <Image
          testID={`miniapp-activity-background-${entry.id}`}
          source={{ uri: mask.uri }}
          contentFit="fill"
          transition={0}
          accessible={false}
          pointerEvents="none"
          style={{
            position: 'absolute',
            left: mask.x,
            top: mask.y + 1,
            width: mask.width,
            height: mask.height,
          }}
        />
      ) : null}
      <Text
        testID={`miniapp-activity-text-${entry.id}`}
        selectable={!canOpen}
        onTextLayout={onTextLayout}
        style={{ color: p.chat.service.text, fontSize: 13, textAlign: 'center' }}
      >
        {attribution ? <Text style={{ fontWeight: '500' }}>{attribution}</Text> : null}
        {[note, title, delivery].filter(Boolean).map((part, index) => (
          <Text key={index}>{`${attribution || index > 0 ? ' · ' : ''}${part}`}</Text>
        ))}
      </Text>
    </>
  );

  return (
    <View
      testID={`miniapp-activity-${activity.entries[0]!.id}`}
      style={{ alignItems: 'center', width: '100%', paddingHorizontal: 16, marginVertical: 4 }}
    >
      {canOpen ? (
        <Pressable
          testID={`miniapp-activity-history-${entry.id}`}
          accessible
          accessibilityRole="button"
          accessibilityLabel={label}
          accessibilityHint={t('miniapps.timelineHistory', { title: edit.title })}
          onPress={() =>
            router.push({ pathname: '/item/[id]', params: { id: edit.item_id, history: '1' } })
          }
          hitSlop={{ top: 11, bottom: 11, left: 8, right: 8 }}
          style={({ pressed }) => ({ ...faceStyle, opacity: pressed ? 0.6 : 1 })}
        >
          {content}
        </Pressable>
      ) : (
        <View
          testID={`miniapp-activity-label-${entry.id}`}
          accessible
          accessibilityRole="text"
          accessibilityLabel={label}
          style={faceStyle}
        >
          {content}
        </View>
      )}
    </View>
  );
}
