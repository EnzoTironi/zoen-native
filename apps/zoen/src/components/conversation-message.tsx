import { memo, useLayoutEffect, useRef, type ComponentRef } from 'react';
import { Text, View, useWindowDimensions } from 'react-native';
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
import { ProfileAvatar } from '../profile/profile-avatar';
import { useTheme } from '../theme';
import { Icon } from './primitives';
import { MessageInteraction, MessagePressable as Pressable } from './message-interaction';
import { MessageReceipt, messageDeliveryLabel } from './message-receipt';
import type { MessageActionMode, MessageAnchor } from './use-message-actions';

export type ConversationMessageProps = {
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
  onAuthor: (entry: TimelineEntry) => void;
  onMenu: (
    entry: TimelineEntry,
    mode: MessageActionMode,
    anchor?: MessageAnchor,
    emoji?: string,
    measureAnchor?: () => Promise<MessageAnchor | null>,
  ) => void;
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
  onAuthor,
  onMenu,
  onQuote,
}: ConversationMessageProps) {
  const { colors } = useTheme();
  const { width } = useWindowDimensions();
  const bubble = useRef<ComponentRef<typeof View>>(null);
  const renderedMessage = useRef<string | null>(entry.id);
  const pendingMeasures = useRef(new Set<(anchor: MessageAnchor | null) => void>());
  useLayoutEffect(() => {
    const measures = pendingMeasures.current;
    renderedMessage.current = entry.id;
    return () => {
      renderedMessage.current = null;
      for (const finish of measures) finish(null);
      measures.clear();
    };
  }, [entry.id]);
  const mine = entry.author.is_me;
  const card = messageCard(entry);
  const voice = !entry.deleted && entry.voice;
  const attachment = Boolean(voice || card);
  const grouped = messagesAreGrouped(previous, entry);
  const lastInGroup = !next || !messagesAreGrouped(entry, next);
  const authorGutter = mine ? 0 : 48;
  const showAvatar = !mine && (lastInGroup || entry.author.kind === 'Agent');
  const authorDisabled = disabled || !messageCanInteract(entry);
  const text = messageText(entry);
  const fileCaption = !voice && card?.kind_id === 'file' && text.trim() ? text : null;
  const ink = mine ? colors.onMine : colors.text;
  const paper = mine ? colors.mine : colors.theirs;
  const availableWidth = Math.max(0, width - 32 - authorGutter);
  const mediaWidth = Math.min(availableWidth, Math.max(160, Math.min(294, availableWidth * 0.87)));
  const time = new Date(Number(entry.at_ms)).toLocaleTimeString(formatLocale, {
    hour: '2-digit',
    minute: '2-digit',
  });
  // VoiceOver groups text bubbles, so their nested quote and metadata must also
  // be represented on the group. Native media keeps its independent controls.
  const accessibilityMessageLabel = [
    `${entry.author.name}: ${text}`,
    !entry.deleted && entry.reply_to
      ? `${t('chat.replyingTo', { name: entry.reply_to.author.name })}: ${entry.reply_to.text}`
      : null,
    !entry.deleted && entry.forwarded ? t('chat.forwarded') : null,
    entry.edited ? t('chat.editedAt', { time }) : time,
    mine ? messageDeliveryLabel(entry.delivery) : null,
  ]
    .filter(Boolean)
    .join('. ');
  const open = (mode: MessageActionMode, emoji?: string) => {
    if (disabled || !messageCanInteract(entry)) return;
    // The keyboard can move this row before the popover is presented. Measure
    // only after it closes, and reject a row recycled while that was pending.
    const measureAnchor = () =>
      new Promise<MessageAnchor | null>((resolve) => {
        const finish = (anchor: MessageAnchor | null) => {
          pendingMeasures.current.delete(finish);
          resolve(anchor);
        };
        pendingMeasures.current.add(finish);
        const view = bubble.current;
        if (!view || renderedMessage.current !== entry.id) return finish(null);
        view.measureInWindow((x, y, measuredWidth, height) =>
          finish(
            renderedMessage.current === entry.id &&
              bubble.current === view &&
              measuredWidth > 0 &&
              height > 0
              ? { x, y, width: measuredWidth, height }
              : null,
          ),
        );
      });
    onMenu(entry, mode, undefined, emoji, measureAnchor);
  };
  const openAuthor = () => {
    if (!authorDisabled) onAuthor(entry);
  };
  // Native media owns its touch stream; a Pressable ancestor can still receive
  // AVKit taps even when the row pan is correctly fenced by a native gesture.
  const Bubble = attachment ? View : Pressable;
  const bubbleInteraction = attachment
    ? { accessible: false }
    : {
        accessible: true,
        accessibilityRole: entry.deleted ? ('text' as const) : ('button' as const),
        accessibilityLabel: accessibilityMessageLabel,
        accessibilityHint:
          disabled || !messageCanInteract(entry)
            ? undefined
            : t('chat.touchAndHoldToReplyOrSeeActions'),
        onPress: () => open('reactions'),
        onLongPress: () => open('actions'),
        onAccessibilityTap: () => open('actions'),
        delayLongPress: 350,
        disabled: disabled || !messageCanInteract(entry),
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
        <View style={{ width: '100%', paddingLeft: authorGutter }}>
          <Pressable
            testID={`message-author-name-${entry.id}`}
            accessibilityRole="button"
            accessibilityLabel={t('chat.viewPerson', { name: entry.author.name })}
            accessibilityState={{ disabled: authorDisabled }}
            disabled={authorDisabled}
            onPress={openAuthor}
            style={({ pressed }) => ({
              minWidth: 44,
              minHeight: 44,
              maxWidth: '100%',
              alignSelf: 'flex-start',
              justifyContent: 'center',
              paddingHorizontal: 12,
              opacity: pressed ? 0.6 : 1,
            })}
          >
            <Text
              numberOfLines={1}
              style={{ color: colors.secondary, fontWeight: '600', fontSize: 12 }}
            >
              {entry.author.name}
            </Text>
          </Pressable>
        </View>
      ) : null}
      <View style={{ width: '100%', flexDirection: 'row', alignItems: 'flex-end' }}>
        {!mine ? (
          <View style={{ width: authorGutter, paddingRight: 4 }}>
            {showAvatar ? (
              <Pressable
                testID={`message-author-avatar-${entry.id}`}
                accessibilityRole="button"
                accessibilityLabel={t('chat.viewPerson', { name: entry.author.name })}
                accessibilityState={{ disabled: authorDisabled }}
                disabled={authorDisabled}
                onPress={openAuthor}
                style={({ pressed }) => ({
                  width: 44,
                  height: 44,
                  alignItems: 'center',
                  justifyContent: 'flex-end',
                  opacity: pressed ? 0.6 : 1,
                })}
              >
                <View
                  accessible={false}
                  accessibilityElementsHidden
                  importantForAccessibility="no-hide-descendants"
                >
                  <ProfileAvatar
                    person={entry.author}
                    name={entry.author.name}
                    color={entry.author.tint_hex}
                    agent={entry.author.kind === 'Agent'}
                    size={36}
                  />
                </View>
              </Pressable>
            ) : null}
          </View>
        ) : null}
        <View style={{ flex: 1, minWidth: 0 }}>
          <MessageInteraction
            messageId={entry.id}
            mine={mine}
            disabled={disabled || !messageCanInteract(entry)}
            accessibilityLabel={!attachment ? accessibilityMessageLabel : undefined}
            onReply={() => onReply(entry)}
            onThread={() => onThread(entry)}
            onActions={() => open('actions')}
            actionsAccessibilityLabel={t('chat.messageActions', { name: entry.author.name })}
            onQuote={
              !entry.deleted && entry.reply_to ? () => onQuote(entry.reply_to!.id) : undefined
            }
          >
            <View style={{ width: '100%', alignItems: mine ? 'flex-end' : 'flex-start' }}>
              <Bubble
                {...bubbleInteraction}
                ref={bubble}
                testID={`message-bubble-${entry.id}`}
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
                    testID={`message-body-${entry.id}`}
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
                {fileCaption ? (
                  <Pressable
                    testID={`message-caption-${entry.id}`}
                    accessible
                    accessibilityRole="button"
                    accessibilityLabel={`${entry.author.name}: ${fileCaption}`}
                    accessibilityHint={
                      disabled ? undefined : t('chat.touchAndHoldToReplyOrSeeActions')
                    }
                    disabled={disabled}
                    onPress={() => open('reactions')}
                    onLongPress={() => open('actions')}
                    onAccessibilityTap={() => open('actions')}
                    delayLongPress={350}
                    style={({ pressed }) => ({
                      backgroundColor: paper,
                      borderRadius: 22,
                      borderCurve: 'continuous',
                      borderBottomRightRadius: mine && lastInGroup ? 6 : 22,
                      borderBottomLeftRadius: !mine && lastInGroup ? 6 : 22,
                      marginTop: 6,
                      paddingHorizontal: 14,
                      paddingVertical: 10,
                      minHeight: 44,
                      opacity: pressed ? 0.8 : 1,
                    })}
                  >
                    <Text style={{ color: ink, fontSize: 17, lineHeight: 23 }}>{fileCaption}</Text>
                  </Pressable>
                ) : null}
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
              </Bubble>
              {attachment && mine ? metadata(true) : null}
            </View>
          </MessageInteraction>
        </View>
      </View>
      <View
        style={{
          width: '100%',
          paddingLeft: authorGutter,
          alignItems: mine ? 'flex-end' : 'flex-start',
        }}
      >
        {!mine && attachment ? (
          <View style={{ width: mediaWidth, maxWidth: '100%' }}>{metadata(true)}</View>
        ) : null}
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
                testID={`message-reaction-${entry.id}-${reaction.emoji}`}
                accessibilityRole="button"
                accessibilityState={{
                  selected: reaction.mine,
                  disabled: disabled || reactionPending,
                }}
                accessibilityLabel={t('messageReactions.participants', {
                  emoji: reaction.emoji,
                  count: reaction.people.length,
                })}
                disabled={disabled || reactionPending}
                onPress={() => open('participants', reaction.emoji)}
                onLongPress={() => open('reactions')}
                style={({ pressed }) => ({
                  minWidth: 44,
                  minHeight: 44,
                  alignItems: 'center',
                  justifyContent: 'center',
                  opacity: pressed || reactionPending ? 0.6 : 1,
                })}
              >
                <View
                  style={{
                    minWidth: 38,
                    minHeight: 32,
                    paddingHorizontal: 9,
                    alignItems: 'center',
                    justifyContent: 'center',
                    borderRadius: 17,
                    borderWidth: 1,
                    borderColor: reaction.mine ? colors.accent : colors.line,
                    backgroundColor: colors.surface,
                  }}
                >
                  <Text style={{ color: colors.text, fontSize: 14 }}>
                    {reaction.emoji}
                    {reaction.people.length > 1 ? ` ${reaction.people.length}` : ''}
                  </Text>
                </View>
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
    </View>
  );
});
