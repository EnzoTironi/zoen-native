import { memo, useRef, type ComponentRef } from 'react';
import { Pressable, Text, View, useWindowDimensions } from 'react-native';
import type { TimelineEntry } from '../core/types';
import type { ItemDetail } from '../core/generated/ItemDetail';
import {
  isSystemMessage,
  messageCanInteract,
  messageCard,
  messagesAreGrouped,
} from '../core/message-presentation';
import { messageText } from '../core/presentation';
import { t, formatLocale } from '../i18n/index.ts';
import { MessageMedia } from '../media/MediaAttachment';
import { MiniappMessageCard } from '../miniapps/message-card';
import { useTheme } from '../theme';
import { Icon } from './primitives';
import { MessageInteraction } from './message-interaction';
import { MessageReceipt, messageDeliveryLabel } from './message-receipt';
import type { MessageActionMode, MessageAnchor } from './use-message-actions';

type Props = {
  spaceId: string;
  entry: TimelineEntry;
  previous?: TimelineEntry;
  next?: TimelineEntry;
  groupChat?: boolean;
  item?: ItemDetail;
  highlighted?: boolean;
  disabled?: boolean;
  reactionPending?: boolean;
  onReply: (entry: TimelineEntry) => void;
  onThread: (entry: TimelineEntry) => void;
  onMenu: (entry: TimelineEntry, mode: MessageActionMode, anchor?: MessageAnchor) => void;
  onReact: (entry: TimelineEntry, emoji: string | null) => void;
  onQuote: (id: string) => void;
};

/** One message face in the main chat and in threads; no alternate, reduced renderer. */
export const ConversationMessage = memo(function ConversationMessage({
  spaceId,
  entry,
  previous,
  next,
  groupChat = false,
  item,
  highlighted = false,
  disabled = false,
  reactionPending = false,
  onReply,
  onThread,
  onMenu,
  onReact,
  onQuote,
}: Props) {
  const { colors } = useTheme();
  const { width } = useWindowDimensions();
  const bubble = useRef<ComponentRef<typeof View>>(null);
  const mine = entry.author.is_me;
  const card = messageCard(entry);
  const voice = !entry.deleted && entry.voice;
  const attachment = Boolean(voice || card);
  const grouped = messagesAreGrouped(previous, entry);
  const lastInGroup = !next || !messagesAreGrouped(entry, next);
  const text = messageText(entry);
  const ink = mine ? colors.onMine : colors.text;
  const paper = mine ? colors.mine : colors.theirs;
  const mediaWidth = Math.max(160, Math.min(294, (width - 32) * 0.87));
  const time = new Date(Number(entry.at_ms)).toLocaleTimeString(formatLocale, {
    hour: '2-digit',
    minute: '2-digit',
  });
  const open = (mode: MessageActionMode) => {
    if (disabled || !messageCanInteract(entry)) return;
    const view = bubble.current;
    if (view)
      view.measureInWindow((x, y, measuredWidth, height) =>
        onMenu(entry, mode, { x, y, width: measuredWidth, height }),
      );
    else onMenu(entry, mode);
  };
  const metadata = (outside = false) => (
    <View
      pointerEvents={outside ? 'auto' : 'none'}
      style={{
        flexDirection: 'row',
        justifyContent: 'flex-end',
        alignItems: 'center',
        gap: 5,
        paddingHorizontal: outside ? 8 : 0,
        paddingTop: 4,
        paddingBottom: outside ? 3 : 0,
      }}
    >
      <Text
        style={{
          color: outside ? colors.secondary : ink,
          opacity: outside ? 1 : 0.65,
          fontSize: 11,
          lineHeight: 14,
          fontVariant: ['tabular-nums'],
        }}
      >
        {entry.edited ? t('chat.editedAt', { time }) : time}
      </Text>
      {mine ? (
        <MessageReceipt
          delivery={entry.delivery}
          ink={outside ? colors.secondary : ink}
          background={outside ? colors.background : paper}
        />
      ) : null}
      {outside && !disabled ? (
        <Pressable
          accessibilityRole="button"
          accessibilityLabel={t('chat.messageActions', { name: entry.author.name })}
          hitSlop={8}
          onPress={() => open('actions')}
          style={({ pressed }) => ({
            width: 28,
            height: 28,
            alignItems: 'center',
            justifyContent: 'center',
            opacity: pressed ? 0.5 : 1,
          })}
        >
          <Icon
            name={{ ios: 'ellipsis', android: 'more_horiz', web: 'more_horiz' }}
            size={14}
            color={colors.secondary}
          />
        </Pressable>
      ) : null}
    </View>
  );
  if (isSystemMessage(entry))
    return (
      <Text
        selectable
        style={{
          color: colors.secondary,
          fontSize: 12,
          lineHeight: 19,
          textAlign: 'center',
          alignSelf: 'center',
          maxWidth: '95%',
          marginVertical: 14,
          paddingHorizontal: 12,
          paddingVertical: 6,
          borderRadius: 12,
          borderWidth: highlighted ? 2 : 0,
          borderColor: colors.accent,
        }}
      >
        {text}
      </Text>
    );
  return (
    <View
      testID={`message-row-${entry.id}`}
      style={{
        alignItems: mine ? 'flex-end' : 'flex-start',
        marginTop: grouped ? 3 : 14,
        marginBottom: lastInGroup ? 5 : 0,
      }}
    >
      {!mine && !grouped && groupChat ? (
        <Text
          style={{
            color: colors.secondary,
            fontWeight: '600',
            fontSize: 12,
            paddingHorizontal: 12,
            paddingBottom: 5,
          }}
        >
          {entry.author.name}
        </Text>
      ) : null}
      <MessageInteraction
        messageId={entry.id}
        mine={mine}
        disabled={disabled || !messageCanInteract(entry)}
        accessibilityLabel={!attachment ? `${entry.author.name}: ${text}` : undefined}
        onReply={() => onReply(entry)}
        onThread={() => onThread(entry)}
        onActions={() => open('actions')}
      >
        <View style={{ width: '100%', alignItems: mine ? 'flex-end' : 'flex-start' }}>
          <Pressable
            ref={bubble}
            testID={`message-bubble-${entry.id}`}
            accessible={!attachment}
            accessibilityRole={entry.deleted ? 'text' : 'button'}
            accessibilityLabel={
              `${entry.author.name}: ${text}` +
              (mine ? `, ${messageDeliveryLabel(entry.delivery)}` : '')
            }
            accessibilityHint={disabled ? undefined : t('chat.touchAndHoldToReplyOrSeeActions')}
            onPress={() => open('reactions')}
            onLongPress={() => open('actions')}
            delayLongPress={350}
            disabled={disabled || !messageCanInteract(entry)}
            style={{
              width: attachment ? mediaWidth : undefined,
              maxWidth: attachment ? '100%' : '87%',
              borderRadius: 22,
              borderCurve: 'continuous',
              borderBottomRightRadius: mine && lastInGroup && !attachment ? 6 : 22,
              borderBottomLeftRadius: !mine && lastInGroup && !attachment ? 6 : 22,
              backgroundColor: attachment && !voice ? 'transparent' : paper,
              paddingHorizontal: attachment ? 0 : 14,
              paddingTop: attachment ? 0 : 9,
              paddingBottom: attachment ? 0 : 7,
            }}
          >
            {!entry.deleted && entry.reply_to ? (
              <Pressable
                accessibilityRole="button"
                accessibilityLabel={t('chat.goToQuotedMessage')}
                disabled={disabled}
                onPress={() => onQuote(entry.reply_to!.id)}
                style={{
                  borderLeftWidth: 3,
                  borderLeftColor: colors.accent,
                  paddingLeft: 9,
                  paddingVertical: 5,
                  marginHorizontal: attachment ? 12 : 0,
                  marginTop: attachment ? 8 : 0,
                  marginBottom: 7,
                }}
              >
                <Text
                  numberOfLines={1}
                  style={{
                    color: attachment && !voice ? colors.text : ink,
                    fontSize: 12,
                    fontWeight: '600',
                  }}
                >
                  {entry.reply_to.author.name}
                </Text>
                <Text
                  numberOfLines={2}
                  style={{
                    color: attachment && !voice ? colors.secondary : ink,
                    fontSize: 13,
                    opacity: 0.75,
                  }}
                >
                  {entry.reply_to.text}
                </Text>
              </Pressable>
            ) : null}
            {!entry.deleted && entry.forwarded ? (
              <Text
                style={{
                  color: attachment && !voice ? colors.secondary : ink,
                  opacity: 0.7,
                  fontSize: 12,
                  fontStyle: 'italic',
                  paddingHorizontal: attachment ? 12 : 0,
                  paddingVertical: 4,
                }}
              >
                {t('chat.forwarded')}
              </Text>
            ) : null}
            {!voice && card?.kind_id !== 'file' && text.trim() ? (
              <Text
                selectable
                style={{
                  color: attachment ? colors.text : ink,
                  fontSize: 17,
                  lineHeight: 23,
                  fontStyle: entry.deleted ? 'italic' : 'normal',
                  paddingHorizontal: attachment ? 12 : 0,
                  paddingVertical: attachment ? 8 : 0,
                }}
              >
                {text}
              </Text>
            ) : null}
            <MessageMedia entry={entry} onActions={() => open('actions')} />
            {card && card.kind_id !== 'file' ? (
              <MiniappMessageCard
                card={card}
                item={item}
                spaceId={spaceId}
                width={mediaWidth}
                onLongPress={() => open('actions')}
              />
            ) : null}
            {!attachment ? metadata() : null}
            {highlighted ? (
              <View
                pointerEvents="none"
                accessible={false}
                style={{
                  position: 'absolute',
                  inset: 0,
                  borderWidth: 2,
                  borderColor: colors.accent,
                  borderRadius: attachment ? 24 : 22,
                  borderBottomRightRadius: mine && lastInGroup && !attachment ? 6 : undefined,
                  borderBottomLeftRadius: !mine && lastInGroup && !attachment ? 6 : undefined,
                }}
              />
            ) : null}
          </Pressable>
          {attachment ? metadata(true) : null}
        </View>
      </MessageInteraction>
      {entry.reactions.length && !entry.deleted ? (
        <View
          style={{
            flexDirection: 'row',
            flexWrap: 'wrap',
            gap: 4,
            paddingHorizontal: 6,
            paddingTop: 2,
            maxWidth: '90%',
          }}
        >
          {entry.reactions.map((reaction) => (
            <Pressable
              key={reaction.emoji}
              accessibilityRole="button"
              accessibilityState={{
                selected: reaction.mine,
                disabled: disabled || reactionPending,
              }}
              accessibilityLabel={t('chat.reactionPeople', {
                emoji: reaction.emoji,
                count: reaction.people.length,
              })}
              disabled={disabled || reactionPending}
              hitSlop={5}
              onPress={() => onReact(entry, reaction.mine ? null : reaction.emoji)}
              onLongPress={() => open('reactions')}
              style={({ pressed }) => ({
                minWidth: 38,
                minHeight: 32,
                paddingHorizontal: 9,
                alignItems: 'center',
                justifyContent: 'center',
                borderRadius: 17,
                borderWidth: 1,
                borderColor: reaction.mine ? colors.accent : colors.line,
                backgroundColor: colors.surface,
                opacity: pressed || reactionPending ? 0.6 : 1,
              })}
            >
              <Text style={{ color: colors.text, fontSize: 14 }}>
                {reaction.emoji}
                {reaction.people.length > 1 ? ` ${reaction.people.length}` : ''}
              </Text>
            </Pressable>
          ))}
        </View>
      ) : null}
      {entry.thread_replies > 0 && !entry.in_thread ? (
        <Pressable
          accessibilityRole="button"
          accessibilityLabel={t('chat.viewReplies', { count: entry.thread_replies })}
          disabled={disabled}
          onPress={() => onThread(entry)}
          style={({ pressed }) => ({
            minHeight: 44,
            flexDirection: 'row',
            alignItems: 'center',
            gap: 6,
            paddingHorizontal: 10,
            opacity: pressed ? 0.6 : 1,
          })}
        >
          <Icon
            name={{ ios: 'bubble.left.and.bubble.right', android: 'forum', web: 'forum' }}
            size={14}
            color={colors.accent}
          />
          <Text style={{ color: colors.accent, fontSize: 13, fontWeight: '600' }}>
            {t('chat.replyCount', { count: entry.thread_replies })}
          </Text>
          <Icon
            name={{ ios: 'chevron.right', android: 'chevron_right', web: 'chevron_right' }}
            size={10}
            color={colors.accent}
          />
        </Pressable>
      ) : null}
    </View>
  );
});
