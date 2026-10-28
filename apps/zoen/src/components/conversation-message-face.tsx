import { Text, View } from 'react-native';
import { isSystemMessage, messageCanInteract } from '../core/message-presentation';
import { t } from '../i18n/index.ts';
import { MessageMedia } from '../media/MediaAttachment';
import { MiniappMessageCard } from '../miniapps/message-card';
import { ProfileAvatar } from '../profile/profile-avatar';
import { MessageContextControl, MessageContextSource } from './message-context-source';
import { MessageBubbleGraphic } from './message-graphics';
import { MessageInteraction, MessagePressable as Pressable } from './message-interaction';
import { MessageMarkdown, MessageTextSelectionLeaf } from './message-markdown';
import type { useConversationMessage } from './use-conversation-message';
type MessageFace = ReturnType<typeof useConversationMessage>;

import {
  renderAlbumMessagePart,
  renderMessageDetails,
  renderMessageMetadata,
  renderServiceMessage,
} from './conversation-message-details';

/** Presentation only; row measurements, recycle fences and gesture callbacks belong to the hook. */
export function renderConversationMessage(context: MessageFace) {
  const {
    entry,
    appCard,
    mine,
    pixel,
    colors,
    face,
    albumContent,
    ink,
    paper,
    attachment,
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
    accessibilityText,
    replyPreview,
    formattedText,
    p,
    Math,
    mediaWidth,
    authorGutter,
    standaloneAppCard,
    item,
    text,
    highlighted,
    grouped,
    showAvatar,
    authorDisabled,
    openAuthor,
    openQuote,
    textMaxWidth,
    attachmentWidth,
    Bubble,
    bubbleInteraction,
    textMinWidth,
    textCorners,
    voice,
    emojiFont,
    lastInGroup,
    groupChat,
    card,
    messageFontSize,
    onTextLayout,
    onTextFrame,
    onImageDecoded,
    revealAnchor,
  } = context;

  const metadata = (outside = false) => renderMessageMetadata(context, outside);
  if (albumPart) return renderAlbumMessagePart(context);
  const details = renderMessageDetails(context, metadata);
  if (isSystemMessage(entry) || 'Request' in entry.kind || 'ItemEdited' in entry.kind)
    return renderServiceMessage(context, details);
  return (
    <View
      testID={`message-row-${entry.id}`}
      style={{
        alignItems: mine ? 'flex-end' : 'flex-start',
        marginTop: grouped ? 0 : 2 + pixel,
        marginBottom: 0,
      }}
    >
      <View style={{ width: '100%', flexDirection: 'row', alignItems: 'flex-end' }}>
        {authorGutter > 0 ? (
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
                    size={38}
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
            swipeEnabled={!albumContent}
            accessibilityLabel={!attachment ? accessibilityMessageLabel : undefined}
            onReply={() => onReply(entry)}
            onThread={() => onThread(entry)}
            onActions={() => open('actions')}
            actionsAccessibilityLabel={t('chat.messageActions', { name: entry.author.name })}
            onQuote={
              !entry.deleted && entry.reply_to && entry.reply_to.origin?.available !== false
                ? openQuote
                : undefined
            }
          >
            <View style={{ width: '100%', alignItems: mine ? 'flex-end' : 'flex-start' }}>
              <MessageContextSource
                ownerKey={JSON.stringify([spaceId, entry.id])}
                active={!disabled && messageCanInteract(entry) && !albumContent}
                handlesTap={!attachment}
                reducedMotion={reducedMotion}
                style={{
                  maxWidth: attachment ? '100%' : textMaxWidth,
                  width: attachment ? attachmentWidth : undefined,
                }}
                onGesture={(request) =>
                  open(request.kind === 'hold' ? 'actions' : 'reactions', undefined, request)
                }
              >
                <Bubble
                  {...bubbleInteraction}
                  ref={bubble}
                  testID={`${standaloneAppCard ? 'message-app-card' : albumContent ? 'message-album-bubble' : 'message-bubble'}-${entry.id}`}
                  style={{
                    width: attachment ? attachmentWidth : undefined,
                    maxWidth: attachment ? '100%' : textMaxWidth,
                    minWidth: attachment ? undefined : textMinWidth,
                    minHeight: attachment ? undefined : 35,
                    ...(attachment && !appCard ? { borderRadius: 22 } : textCorners),
                    borderCurve: 'continuous',
                    backgroundColor: attachment && voice ? paper : 'transparent',
                    paddingHorizontal: attachment ? 0 : 11,
                    paddingLeft: attachment ? 0 : mine ? 11 : 17,
                    paddingRight: attachment ? 0 : mine ? 17 : 11,
                    paddingTop: attachment ? 0 : emojiFont ? 16 : 6 + pixel,
                    paddingBottom: attachment ? 0 : emojiFont ? 20 : 6 - pixel,
                  }}
                >
                  {(!attachment || (appCard && !standaloneAppCard)) && !emojiFont ? (
                    <MessageBubbleGraphic
                      ownerKey={entry.id}
                      incoming={!mine}
                      above={grouped}
                      below={!lastInGroup}
                      fill={paper}
                      fillColors={face.fillColors}
                    />
                  ) : null}
                  {!mine && !grouped && groupChat && !standaloneAppCard ? (
                    <MessageContextControl controlKey={`author:${entry.id}`}>
                      <Pressable
                        testID={`message-author-name-${entry.id}`}
                        accessibilityRole="button"
                        accessibilityLabel={t('chat.viewPerson', { name: entry.author.name })}
                        accessibilityState={{ disabled: authorDisabled }}
                        disabled={authorDisabled}
                        onPress={openAuthor}
                        hitSlop={{ top: 10, bottom: 10, left: 4, right: 4 }}
                        style={({ pressed }) => ({
                          alignSelf: 'flex-start',
                          maxWidth: '100%',
                          marginBottom: 2,
                          ...(appCard ? { marginHorizontal: 11, marginTop: 7 } : {}),
                          opacity: pressed ? 0.6 : 1,
                        })}
                      >
                        <Text
                          numberOfLines={1}
                          style={{ color: face.accent, fontWeight: '600', fontSize: 13 }}
                        >
                          {entry.author.name}
                        </Text>
                      </Pressable>
                    </MessageContextControl>
                  ) : null}
                  {!entry.deleted && entry.reply_to ? (
                    <MessageContextControl controlKey={`quote:${entry.id}`}>
                      <Pressable
                        accessibilityRole={
                          entry.reply_to.origin?.available === false ? 'text' : 'button'
                        }
                        accessibilityLabel={
                          entry.reply_to.origin?.available === false
                            ? undefined
                            : t('chat.goToQuotedMessage')
                        }
                        disabled={disabled || entry.reply_to.origin?.available === false}
                        onPress={openQuote}
                        style={{
                          borderLeftWidth: 3,
                          borderLeftColor: face.accent,
                          backgroundColor: face.reaction,
                          borderRadius: 4,
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
                            color: attachment && !voice && !appCard ? colors.text : ink,
                            fontSize: 14,
                            fontWeight: '600',
                          }}
                        >
                          {entry.reply_to.author.name}
                        </Text>
                        <Text
                          numberOfLines={2}
                          style={{
                            color: attachment && !voice && !appCard ? colors.secondary : ink,
                            fontSize: 14,
                            opacity: 0.75,
                          }}
                        >
                          {replyPreview.text ?? '…'}
                        </Text>
                      </Pressable>
                    </MessageContextControl>
                  ) : null}
                  {!entry.deleted && entry.forwarded ? (
                    <Text
                      style={{
                        color: attachment && !voice && !appCard ? colors.secondary : ink,
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
                  {!voice && !standaloneAppCard && card?.kind_id !== 'file' && text.trim() ? (
                    formattedText ? (
                      <View
                        style={{
                          paddingHorizontal: appCard ? 11 : attachment ? 12 : 0,
                          paddingVertical: appCard ? 6 : attachment ? 8 : 0,
                        }}
                      >
                        <MessageMarkdown
                          revision={entry.text_revision}
                          expandedForReveal={Boolean(revealAnchor)}
                          testID={`message-body-${entry.id}`}
                          onLayout={revealAnchor?.onLayout}
                          text={text}
                          accessibilityText={accessibilityText}
                          color={attachment && !appCard ? colors.text : ink}
                          fontSize={messageFontSize}
                          maxWidth={Math.max(
                            1,
                            attachment ? attachmentWidth - (appCard ? 22 : 24) : textMaxWidth - 28,
                          )}
                        />
                      </View>
                    ) : (
                      <MessageTextSelectionLeaf
                        revision={entry.text_revision}
                        expandedForReveal={Boolean(revealAnchor)}
                        text={text}
                        color={attachment && !appCard ? colors.text : ink}
                        fontSize={emojiFont ?? messageFontSize}
                        maxWidth={Math.max(
                          1,
                          attachment ? attachmentWidth - (appCard ? 22 : 24) : textMaxWidth - 28,
                        )}
                        onLayout={revealAnchor?.onLayout}
                      >
                        <Text
                          testID={`message-body-${entry.id}`}
                          onTextLayout={
                            revealAnchor
                              ? (event) => {
                                  if (!attachment) onTextLayout(event);
                                  revealAnchor.onTextLayout(event);
                                }
                              : attachment
                                ? undefined
                                : onTextLayout
                          }
                          onLayout={
                            revealAnchor
                              ? (event) => {
                                  if (!attachment) onTextFrame(event);
                                  revealAnchor.onLayout(event);
                                }
                              : attachment
                                ? undefined
                                : onTextFrame
                          }
                          style={{
                            color: attachment && !appCard ? colors.text : ink,
                            fontSize: messageFontSize,
                            lineHeight: attachment ? Math.ceil(messageFontSize * 1.3) : undefined,
                            fontStyle: entry.deleted ? 'italic' : 'normal',
                            paddingHorizontal: appCard ? 11 : attachment ? 12 : 0,
                            paddingVertical: appCard ? 6 : attachment ? 8 : 0,
                          }}
                        >
                          {text}
                        </Text>
                      </MessageTextSelectionLeaf>
                    )
                  ) : null}
                  {albumContent ? (
                    albumContent.media
                  ) : (
                    <MessageMedia
                      spaceId={spaceId}
                      entry={entry}
                      onActions={() => open('actions')}
                      onImageDecoded={onImageDecoded}
                    />
                  )}
                  {albumContent?.captions}
                  {!albumContent && fileCaption ? (
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
                        experimental_backgroundImage:
                          face.fillColors.length > 1
                            ? `linear-gradient(to bottom, ${face.fillColors.join(', ')})`
                            : undefined,
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
                      {formattedText ? (
                        <MessageMarkdown
                          revision={entry.text_revision}
                          expandedForReveal={Boolean(revealAnchor)}
                          text={fileCaption}
                          accessibilityText={accessibilityText}
                          onLayout={revealAnchor?.onLayout}
                          color={ink}
                          fontSize={p.typography.message.fontSize}
                          maxWidth={Math.max(1, mediaWidth - 28)}
                        />
                      ) : (
                        <MessageTextSelectionLeaf
                          revision={entry.text_revision}
                          expandedForReveal={Boolean(revealAnchor)}
                          text={fileCaption!}
                          color={ink}
                          fontSize={p.typography.message.fontSize}
                          maxWidth={Math.max(1, mediaWidth - 28)}
                        >
                          <Text
                            onLayout={revealAnchor?.onLayout}
                            onTextLayout={revealAnchor?.onTextLayout}
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
                  ) : null}
                  {card && card.kind_id !== 'file' ? (
                    <View style={appCard && !standaloneAppCard ? { margin: 2 } : undefined}>
                      <MiniappMessageCard
                        card={card}
                        item={item}
                        spaceId={spaceId}
                        width={mediaWidth}
                        onLongPress={() => open('actions')}
                      />
                    </View>
                  ) : null}
                  {(!attachment || (appCard && !standaloneAppCard)) && !emojiFont
                    ? metadata()
                    : null}
                  {highlighted ? (
                    <View
                      pointerEvents="none"
                      accessible={false}
                      style={{
                        position: 'absolute',
                        inset: 0,
                        borderWidth: 2,
                        borderColor: colors.accent,
                        ...(standaloneAppCard || (attachment && !appCard)
                          ? { borderRadius: 24 }
                          : textCorners),
                      }}
                    />
                  ) : null}
                </Bubble>
              </MessageContextSource>
              {(attachment && (!appCard || standaloneAppCard) && mine) || emojiFont
                ? metadata(true)
                : null}
            </View>
          </MessageInteraction>
        </View>
      </View>
      {details}
    </View>
  );
}
