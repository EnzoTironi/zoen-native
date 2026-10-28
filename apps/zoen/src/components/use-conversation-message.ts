import {
  useCallback,
  useLayoutEffect,
  useRef,
  useState,
  type ComponentRef,
  type ReactNode,
} from 'react';
import {
  View,
  useWindowDimensions,
  type LayoutChangeEvent,
  type TextLayoutEvent,
} from 'react-native';
import type { ItemDetail } from '../core/generated/ItemDetail';
import { useRenderedMessageText, type RenderedMessageText } from './use-rendered-message-text';
import { useSemanticMessagePreview } from './use-semantic-message-preview';
import { messageMayContainMarkdown } from '../core/message-markdown';
import {
  largeEmojiFontSize,
  messageCanInteract,
  messageCard,
  messageHasStandaloneAppCard,
  messagesAreGrouped,
} from '../core/message-presentation';
import { messageText } from '../core/presentation';
import type { TimelineEntry } from '../core/types';
import { formatLocale, t } from '../i18n/index.ts';
import { type MediaTile } from '../media/MediaAttachment';
import { inlineImageFrame, type DecodedMessageImage } from '../media/inline-image-frame';
import { APP_CARD_METRICS } from '../miniapps/app-card-layout';
import { useTheme } from '../theme';
import type { MessageContextRequest } from './message-context-ports';
import { MessagePressable as Pressable } from './message-interaction';
import { messageDeliveryLabel } from './message-receipt';
import type { MessageActionMode, MessageAnchor } from './use-message-actions';
import { useConversationRevealAnchor } from './conversation-reveal-content-view';

export type ConversationMessageProps = {
  spaceId: string;
  accessibilityText?: RenderedMessageText;
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
    galleryEntries?: readonly TimelineEntry[];
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
export function useConversationMessage({
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
  accessibilityText: sharedAccessibilityText,
}: ConversationMessageProps) {
  const { colors, presentation: p, reducedMotion } = useTheme();
  const revealAnchor = useConversationRevealAnchor(entry.id);
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
  const replyPreview = useSemanticMessagePreview({
    owner: JSON.stringify([
      spaceId,
      entry.id,
      entry.text_revision,
      entry.reply_to?.id,
      entry.reply_to?.selection?.revision,
    ]),
    text: entry.reply_to?.selection?.text ?? entry.reply_to?.text ?? '',
    enabled: !entry.deleted && !!entry.reply_to,
    literal: !!entry.reply_to?.selection,
    target:
      entry.reply_to && !entry.reply_to.selection
        ? { spaceId, messageId: entry.reply_to.id }
        : undefined,
  });
  const mine = entry.author.is_me;
  const card = messageCard(entry);
  const appCard = card?.kind_id === 'app' && !!card.app;
  const standaloneAppCard = messageHasStandaloneAppCard(entry);
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
  const fileCaption =
    !voice && !standaloneAppCard && card?.kind_id === 'file' && text.trim() ? text : null;
  const face = mine ? p.chat.outgoing : p.chat.incoming;
  const ink = face.text;
  const paper = face.fill;
  const emojiFont =
    !attachment && !entry.deleted && !entry.reply_to && !entry.forwarded
      ? largeEmojiFontSize(text, p.largeEmoji)
      : null;
  const messageFontSize = emojiFont ?? p.typography.message.fontSize;
  const formattedText =
    !entry.deleted &&
    !standaloneAppCard &&
    !emojiFont &&
    messageMayContainMarkdown(text, process.env.EXPO_OS === 'ios');
  const accessibilityText = useRenderedMessageText(
    JSON.stringify([spaceId, entry.id]),
    text,
    formattedText && process.env.EXPO_OS === 'ios',
    sharedAccessibilityText,
  );
  const availableWidth = Math.max(0, width - 32 - authorGutter);
  const mediaLimit = Math.min(availableWidth, Math.max(160, Math.min(294, availableWidth * 0.87)));
  const mediaWidth = appCard
    ? APP_CARD_METRICS.size
    : decodedImage?.key === imageKey
      ? inlineImageFrame(mediaLimit, decodedImage.width / decodedImage.height).width
      : mediaLimit;
  // Telegram RichDataBubbleContentNode.pageContentInset: retain a 2pt media rim.
  // The cover itself is the exact same 160pt canonical tile used in Home/rails.
  const attachmentWidth =
    albumContent?.width ?? (appCard && !standaloneAppCard ? mediaWidth + 4 : mediaWidth);
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
    !formattedText &&
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
    accessibilityText.text === undefined
      ? entry.author.name
      : `${entry.author.name}: ${accessibilityText.text}`,
    !entry.deleted && entry.reply_to
      ? [t('chat.replyingTo', { name: entry.reply_to.author.name }), replyPreview.text]
          .filter(Boolean)
          .join(': ')
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

  return {
    entry,
    revealAnchor,
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
    reactionPending,
    item,
    text,
    highlighted,
    grouped,
    showAvatar,
    authorDisabled,
    openAuthor,
    onQuote,
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
  };
}
