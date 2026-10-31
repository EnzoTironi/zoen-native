import { messageForwardLabel } from '../core/message-presentation';
import { router } from 'expo-router';
import { type ReactNode } from 'react';
import { Text, View } from 'react-native';
import { messageCanInteract } from '../core/message-presentation';
import { t } from '../i18n/index.ts';
import { MessageMedia } from '../media/MediaAttachment';
import { MessageContextSource } from './message-context-source';
import { MessageInteraction, MessagePressable as Pressable } from './message-interaction';
import { MessageMarkdown, MessageTextSelectionLeaf } from './message-markdown';
import { MessageReceipt } from './message-receipt';
import { Icon } from './primitives';
import { ServiceMessage } from './service-message';
import type { useConversationMessage } from './use-conversation-message';
type MessageFace = ReturnType<typeof useConversationMessage>;

export function renderMessageMetadata(context: MessageFace, outside = false) {
  const {
    entry,
    onStatusLayout,
    appCard,
    inlineStatus,
    mine,
    pixel,
    colors,
    face,
    time,
    albumContent,
    ink,
    paper,
    attachment,
    disabled,
    open,
  } = context;
  return (
    <View
      testID={`message-metadata-${entry.id}`}
      onLayout={outside ? undefined : onStatusLayout}
      pointerEvents={outside ? 'auto' : 'none'}
      style={{
        flexDirection: 'row',
        justifyContent: 'flex-end',
        alignItems: 'center',
        gap: 5,
        paddingHorizontal: outside ? 8 : appCard ? 11 : 0,
        alignSelf: 'flex-end',
        position: !outside && inlineStatus ? 'absolute' : undefined,
        right: !outside && inlineStatus ? (mine ? 17 : 11) : undefined,
        bottom: !outside && inlineStatus ? 6 - pixel : undefined,
        paddingTop: outside ? 4 : appCard ? 4 : inlineStatus ? 0 : 2,
        paddingBottom: outside ? 3 : appCard ? 6 : 0,
      }}
    >
      <Text
        style={{
          color: outside ? colors.secondary : face.secondary,
          fontSize: 11,
          lineHeight: 14,
          fontVariant: ['tabular-nums'],
        }}
      >
        {entry.edited ? t('chat.editedAt', { time }) : time}
      </Text>
      {mine ? (
        <MessageReceipt
          delivery={albumContent?.delivery ?? entry.delivery}
          ownerKey={entry.id}
          ink={outside ? colors.secondary : ink}
          background={outside ? colors.background : paper}
          backgrounds={outside ? undefined : face.fillColors}
        />
      ) : null}
      {attachment &&
      !disabled &&
      ((outside && !appCard) || (appCard && process.env.EXPO_OS !== 'ios')) ? (
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
            color={outside ? colors.secondary : face.secondary}
          />
        </Pressable>
      ) : null}
    </View>
  );
}

export function renderAlbumMessagePart(context: MessageFace) {
  const {
    entry,
    mine,
    ink,
    paper,
    disabled,
    open,
    albumPart,
    fileCaption,
    onReply,
    onThread,
    JSON,
    spaceId,
    reducedMotion,
    bubble,
    accessibilityMessageLabel,
    formattedText,
    p,
    Math,
    mediaWidth,
  } = context;
  if (!albumPart) return null;
  const tile = albumPart.tile;
  if (albumPart.captionOnly && !fileCaption) return null;
  return (
    <MessageInteraction
      messageId={entry.id}
      mine={mine}
      disabled={disabled || !messageCanInteract(entry)}
      onReply={() => onReply(entry)}
      onThread={() => onThread(entry)}
      onActions={() => open('actions')}
      actionsAccessibilityLabel={t('chat.messageActions', { name: entry.author.name })}
    >
      <MessageContextSource
        ownerKey={JSON.stringify([spaceId, entry.id, tile ? 'album-media' : 'album-caption'])}
        active={!disabled && messageCanInteract(entry)}
        handlesTap={false}
        reducedMotion={reducedMotion}
        style={tile ? { width: tile.width, height: tile.height } : { width: '100%' }}
        onGesture={(request) =>
          open(request.kind === 'hold' ? 'actions' : 'reactions', undefined, request)
        }
      >
        <View
          ref={bubble}
          testID={`message-bubble-${entry.id}`}
          style={tile ? { width: tile.width, height: tile.height } : undefined}
        >
          {tile ? (
            <MessageMedia
              spaceId={spaceId}
              entry={entry}
              tile={tile}
              onActions={() => open('actions')}
              onImageDecoded={albumPart.onDecoded}
              onInlineUnavailable={albumPart.onInlineUnavailable}
              galleryEntries={albumPart.galleryEntries}
            />
          ) : (
            <Pressable
              accessibilityRole="button"
              accessibilityLabel={accessibilityMessageLabel}
              disabled={disabled}
              onPress={() => open('reactions')}
              onLongPress={() => open('actions')}
              delayLongPress={350}
              style={{
                backgroundColor: paper,
                paddingHorizontal: 14,
                paddingVertical: 8,
                minHeight: 44,
              }}
            >
              {formattedText ? (
                <MessageMarkdown
                  revision={entry.text_revision}
                  expandedForReveal={Boolean(context.revealAnchor)}
                  testID={`message-caption-${entry.id}`}
                  text={fileCaption!}
                  color={ink}
                  fontSize={p.typography.message.fontSize}
                  maxWidth={Math.max(1, mediaWidth - 28)}
                />
              ) : (
                <MessageTextSelectionLeaf
                  revision={entry.text_revision}
                  expandedForReveal={Boolean(context.revealAnchor)}
                  text={fileCaption!}
                  color={ink}
                  fontSize={p.typography.message.fontSize}
                  maxWidth={Math.max(1, mediaWidth - 28)}
                >
                  <Text
                    testID={`message-caption-${entry.id}`}
                    style={{
                      color: ink,
                      fontSize: p.typography.message.fontSize,
                      lineHeight: Math.ceil(p.typography.message.fontSize * 1.3),
                    }}
                  >
                    {fileCaption}
                  </Text>
                </MessageTextSelectionLeaf>
              )}
            </Pressable>
          )}
        </View>
      </MessageContextSource>
    </MessageInteraction>
  );
}

export function renderMessageDetails(
  context: MessageFace,
  metadata: (outside?: boolean) => ReactNode,
) {
  const {
    entry,
    appCard,
    mine,
    colors,
    attachment,
    disabled,
    open,
    onThread,
    mediaWidth,
    authorGutter,
    standaloneAppCard,
    reactionPending,
  } = context;
  return (
    <View
      style={{
        width: '100%',
        paddingLeft: authorGutter,
        alignItems: mine ? 'flex-end' : 'flex-start',
      }}
    >
      {!mine && attachment && (!appCard || standaloneAppCard) ? (
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
  );
}

export function renderServiceMessage(context: MessageFace, details: ReactNode) {
  const { entry, disabled, spaceId, item, text, highlighted } = context;

  const request = 'Request' in entry.kind ? entry.kind.Request : null;
  const edit = 'ItemEdited' in entry.kind ? entry.kind.ItemEdited : null;
  const canOpenItem =
    !!edit && !entry.deleted && item?.id === edit.item_id && item.space_id === spaceId;
  const serviceLabel = [
    messageForwardLabel(entry),
    !entry.deleted && entry.reply_to ? `${entry.reply_to.author.name}: ${entry.reply_to.text}` : '',
    !entry.deleted && edit ? `${entry.author.name} · ${text} · ${edit.title}` : text,
  ]
    .filter(Boolean)
    .join(' · ');
  const service = (
    <ServiceMessage
      id={entry.id}
      label={serviceLabel}
      highlighted={highlighted}
      accessibilityHint={
        request
          ? t('agents.reviewRequest')
          : canOpenItem
            ? t('miniapps.timelineHistory', { title: edit!.title })
            : undefined
      }
      onPress={
        disabled || entry.deleted
          ? undefined
          : request
            ? () => router.push({ pathname: '/request/[id]', params: { id: request.request_id } })
            : canOpenItem
              ? () =>
                  router.push({
                    pathname: '/item/[id]',
                    params: { id: edit!.item_id, history: '1' },
                  })
              : undefined
      }
    />
  );
  return entry.reactions.length || entry.thread_replies > 0 ? (
    <View style={{ width: '100%' }}>
      {service}
      {details}
    </View>
  ) : (
    service
  );
}
