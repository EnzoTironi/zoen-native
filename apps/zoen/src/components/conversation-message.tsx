import {
  memo,
  useCallback,
  useLayoutEffect,
  useRef,
  useState,
  type ComponentRef,
  type ReactNode,
} from 'react';
import {
  Text,
  View,
  useWindowDimensions,
  type LayoutChangeEvent,
  type TextLayoutEvent,
} from 'react-native';
import type { TimelineEntry } from '../core/types';
import type { ItemDetail } from '../core/generated/ItemDetail';
import {
  isSystemMessage,
  largeEmojiFontSize,
  messageCanInteract,
  messageCard,
  messagesAreGrouped,
} from '../core/message-presentation';
import { messageText } from '../core/presentation';
import { t, formatLocale } from '../i18n/index.ts';
import { MessageMedia, type MediaTile } from '../media/MediaAttachment';
import { inlineImageFrame, type DecodedMessageImage } from '../media/inline-image-frame';
import { MiniappMessageCard } from '../miniapps/message-card';
import { APP_CARD_METRICS } from '../miniapps/app-card-layout';
import { ProfileAvatar } from '../profile/profile-avatar';
import { useTheme } from '../theme';
import { Icon } from './primitives';
import { MessageInteraction, MessagePressable as Pressable } from './message-interaction';
import { MessageReceipt, messageDeliveryLabel } from './message-receipt';
import { MessageBubbleGraphic } from './message-graphics';
import type { MessageActionMode, MessageAnchor } from './use-message-actions';
import { MessageContextSource, MessageContextControl } from './message-context-source';
import type { MessageContextRequest } from './message-context-ports';

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
  albumContent?: {
    width: number;
    media: ReactNode;
    captions: ReactNode;
    delivery: TimelineEntry['delivery'];
  };
  albumPart?: {
    tile?: MediaTile;
    captionOnly?: boolean;
    onDecoded?: (image: DecodedMessageImage) => void;
    onInlineUnavailable?: () => void;
  };
  onReply: (entry: TimelineEntry) => void;
  onThread: (entry: TimelineEntry) => void;
  onAuthor: (entry: TimelineEntry) => void;
  onMenu: (
    entry: TimelineEntry,
    mode: MessageActionMode,
    anchor?: MessageAnchor,
    emoji?: string,
    measureAnchor?: () => Promise<MessageAnchor | null>,
    nativeSource?: MessageContextRequest,
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
  albumContent,
  albumPart,
  onReply,
  onThread,
  onAuthor,
  onMenu,
  onQuote,
}: ConversationMessageProps) {
  const { colors, presentation: p, reducedMotion } = useTheme();
  const { width, scale, fontScale } = useWindowDimensions();
  const pixel = 1 / (Number.isFinite(scale) && scale > 0 ? scale : 1);
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
  const appCard = card?.kind_id === 'app' && !!card.app;
  const imageKey = card?.kind_id === 'file' ? `${card.item_id}:${card.version}` : null;
  const renderedImage = useRef(imageKey);
  const [decodedImage, setDecodedImage] = useState<(DecodedMessageImage & { key: string }) | null>(
    null,
  );
  useLayoutEffect(() => {
    renderedImage.current = imageKey;
    return () => {
      renderedImage.current = null;
    };
  }, [imageKey]);
  const onImageDecoded = useCallback(
    (image: DecodedMessageImage) => {
      if (
        !imageKey ||
        renderedImage.current !== imageKey ||
        image.itemId !== card?.item_id ||
        image.version !== card.version ||
        !Number.isFinite(image.width) ||
        !Number.isFinite(image.height) ||
        image.width <= 0 ||
        image.height <= 0
      )
        return;
      setDecodedImage((previous) =>
        previous?.key === imageKey &&
        previous.sha256 === image.sha256 &&
        previous.width === image.width &&
        previous.height === image.height
          ? previous
          : { ...image, key: imageKey },
      );
    },
    [imageKey, card],
  );
  const voice = !entry.deleted && entry.voice;
  const attachment = Boolean(voice || card);
  const grouped = messagesAreGrouped(previous, entry);
  const lastInGroup = !next || !messagesAreGrouped(entry, next);
  const authorGutter = !mine && groupChat ? 44 : 0;
  const showAvatar = !mine && groupChat && lastInGroup;
  const authorDisabled = disabled || !messageCanInteract(entry);
  const text = messageText(entry);
  const fileCaption = !voice && card?.kind_id === 'file' && text.trim() ? text : null;
  const face = mine ? p.chat.outgoing : p.chat.incoming;
  const ink = face.text;
  const paper = face.fill;
  const emojiFont =
    !attachment && !entry.deleted && !entry.reply_to && !entry.forwarded
      ? largeEmojiFontSize(text, p.largeEmoji)
      : null;
  const messageFontSize = emojiFont ?? p.typography.message.fontSize;
  const availableWidth = Math.max(0, width - 32 - authorGutter);
  const mediaLimit = Math.min(availableWidth, Math.max(160, Math.min(294, availableWidth * 0.87)));
  const mediaWidth = appCard
    ? APP_CARD_METRICS.size
    : decodedImage?.key === imageKey
      ? inlineImageFrame(mediaLimit, decodedImage.width / decodedImage.height).width
      : mediaLimit;
  // Telegram RichDataBubbleContentNode.pageContentInset: retain a 2pt media rim.
  // The cover itself is the exact same 160pt canonical tile used in Home/rails.
  const attachmentWidth = albumContent?.width ?? (appCard ? mediaWidth + 4 : mediaWidth);
  const time = new Date(Number(entry.at_ms)).toLocaleTimeString(formatLocale, {
    hour: '2-digit',
    minute: '2-digit',
  });
  // Telegram's compact text metrics; media keeps its own aspect and card policy.
  // Apply fill to the viewport before Telegram's outer edge residual (3×3)
  // and the retained Zoen author gutter. Row padding only caps the result.
  const textWidthFill =
    width <= 500 ? Math.max(1, width - 36) : Math.max(1, Math.floor(width * 0.85));
  const textMaxWidth = Math.min(availableWidth, Math.max(40, textWidthFill - 9 - authorGutter));
  const layoutKey = `${entry.id.length}:${entry.id}${text.length}:${text}${time}:${entry.edited}:${entry.delivery}:${width}:${fontScale}:${attachment}:${messageFontSize}`;
  const renderedText = useRef<string | null>(layoutKey);
  const [textLayout, setTextLayout] = useState<{
    key: string;
    count: number;
    right: number;
    height: number;
  } | null>(null);
  const [textFrame, setTextFrame] = useState<{ key: string; width: number } | null>(null);
  const [statusLayout, setStatusLayout] = useState<{
    key: string;
    width: number;
    height: number;
  } | null>(null);
  useLayoutEffect(() => {
    renderedText.current = layoutKey;
    return () => {
      renderedText.current = null;
    };
  }, [layoutKey]);
  const onTextLayout = useCallback(
    (event: TextLayoutEvent) => {
      if (renderedText.current !== layoutKey) return;
      const lines = event.nativeEvent.lines;
      const last = lines.at(-1);
      if (
        !last ||
        ![last.x, last.width, last.height].every(Number.isFinite) ||
        last.x < 0 ||
        last.width < 0 ||
        last.height <= 0
      )
        return;
      const right = last.x + last.width;
      setTextLayout((prior) =>
        prior?.key === layoutKey &&
        prior.count === lines.length &&
        prior.right === right &&
        prior.height === last.height
          ? prior
          : { key: layoutKey, count: lines.length, right, height: last.height },
      );
    },
    [layoutKey],
  );
  const onTextFrame = useCallback(
    (event: LayoutChangeEvent) => {
      if (renderedText.current !== layoutKey) return;
      const measured = event.nativeEvent.layout.width;
      if (!Number.isFinite(measured) || measured <= 0) return;
      setTextFrame((prior) =>
        prior?.key === layoutKey && prior.width === measured
          ? prior
          : { key: layoutKey, width: measured },
      );
    },
    [layoutKey],
  );
  const onStatusLayout = useCallback(
    (event: LayoutChangeEvent) => {
      if (renderedText.current !== layoutKey) return;
      const { width: measured, height } = event.nativeEvent.layout;
      if (![measured, height].every(Number.isFinite) || measured <= 0 || height <= 0) return;
      setStatusLayout((prior) =>
        prior?.key === layoutKey && prior.width === measured && prior.height === height
          ? prior
          : { key: layoutKey, width: measured, height },
      );
    },
    [layoutKey],
  );
  const measuredText = textLayout?.key === layoutKey ? textLayout : null;
  const measuredFrame = textFrame?.key === layoutKey ? textFrame : null;
  const measuredStatus = statusLayout?.key === layoutKey ? statusLayout : null;
  // RTL text gets a separate status row, as in Telegram's trailing-line layout.
  const canMeasureInline =
    !attachment &&
    !emojiFont &&
    !!text.trim() &&
    !/[\u0590-\u08ff\ufb1d-\ufdff\ufe70-\ufeff]/u.test(text);
  const singleLineWidth =
    canMeasureInline && measuredText?.count === 1 && measuredStatus
      ? measuredText.right + measuredStatus.width + 5 + 28
      : 40;
  const textMinWidth = Math.min(textMaxWidth, Math.max(40, singleLineWidth));
  const trailingWidth = Math.max(measuredFrame?.width ?? 0, textMinWidth - 28);
  const inlineStatus =
    canMeasureInline &&
    !!measuredText &&
    !!measuredStatus &&
    !!measuredFrame &&
    measuredText.height >= measuredStatus.height &&
    trailingWidth - measuredText.right >= measuredStatus.width + 5;
  const textCorners = {
    borderRadius: 16,
    borderTopRightRadius: mine && grouped ? 8 : 16,
    borderTopLeftRadius: !mine && grouped ? 8 : 16,
    borderBottomRightRadius: mine && !lastInGroup ? 8 : 16,
    borderBottomLeftRadius: !mine && !lastInGroup ? 8 : 16,
  };
  // VoiceOver groups text bubbles, so their nested quote and metadata must also
  // be represented on the group. Native media keeps its independent controls.
  const accessibilityMessageLabel = [
    `${entry.author.name}: ${text}`,
    !entry.deleted && entry.reply_to
      ? `${t('chat.replyingTo', { name: entry.reply_to.author.name })}: ${entry.reply_to.text}`
      : null,
    !entry.deleted && entry.forwarded ? t('chat.forwarded') : null,
    entry.edited ? t('chat.editedAt', { time }) : time,
    mine ? messageDeliveryLabel(albumContent?.delivery ?? entry.delivery) : null,
  ]
    .filter(Boolean)
    .join('. ');
  const open = (mode: MessageActionMode, emoji?: string, nativeSource?: MessageContextRequest) => {
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
    onMenu(entry, mode, undefined, emoji, measureAnchor, nativeSource);
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
      {attachment && !disabled && (outside || (appCard && process.env.EXPO_OS !== 'ios')) ? (
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
  if (albumPart) {
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
              </Pressable>
            )}
          </View>
        </MessageContextSource>
      </MessageInteraction>
    );
  }
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
            disabled={disabled || !messageCanInteract(entry) || Boolean(albumContent)}
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
                  testID={`${albumContent ? 'message-album-bubble' : 'message-bubble'}-${entry.id}`}
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
                  {(!attachment || appCard) && !emojiFont ? (
                    <MessageBubbleGraphic
                      ownerKey={entry.id}
                      incoming={!mine}
                      above={grouped}
                      below={!lastInGroup}
                      fill={paper}
                      fillColors={face.fillColors}
                    />
                  ) : null}
                  {!mine && !grouped && groupChat ? (
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
                        accessibilityRole="button"
                        accessibilityLabel={t('chat.goToQuotedMessage')}
                        disabled={disabled}
                        onPress={() => onQuote(entry.reply_to!.id)}
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
                          {entry.reply_to.text}
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
                  {!voice && card?.kind_id !== 'file' && text.trim() ? (
                    <Text
                      testID={`message-body-${entry.id}`}
                      onTextLayout={attachment ? undefined : onTextLayout}
                      onLayout={attachment ? undefined : onTextFrame}
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
                      <Text
                        style={{
                          color: ink,
                          fontSize: p.typography.message.fontSize,
                          lineHeight: Math.ceil(p.typography.message.fontSize * 1.3),
                        }}
                      >
                        {fileCaption}
                      </Text>
                    </Pressable>
                  ) : null}
                  {card && card.kind_id !== 'file' ? (
                    <View style={appCard ? { margin: 2 } : undefined}>
                      <MiniappMessageCard
                        card={card}
                        item={item}
                        spaceId={spaceId}
                        width={mediaWidth}
                        onLongPress={() => open('actions')}
                      />
                    </View>
                  ) : null}
                  {(!attachment || appCard) && !emojiFont ? metadata() : null}
                  {highlighted ? (
                    <View
                      pointerEvents="none"
                      accessible={false}
                      style={{
                        position: 'absolute',
                        inset: 0,
                        borderWidth: 2,
                        borderColor: colors.accent,
                        ...(attachment && !appCard ? { borderRadius: 24 } : textCorners),
                      }}
                    />
                  ) : null}
                </Bubble>
              </MessageContextSource>
              {(attachment && !appCard && mine) || emojiFont ? metadata(true) : null}
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
        {!mine && attachment && !appCard ? (
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
