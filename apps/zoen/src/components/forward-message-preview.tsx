import { forwardPreviewEntry, DEFAULT_FORWARD_OPTIONS } from '../core/forward-options';
import type { ForwardOptions } from '../core/generated/ForwardOptions';
import { Text, View, useWindowDimensions } from 'react-native';
import type { TimelineEntry } from '../core/types';
import { MessageMedia } from '../media/MediaAttachment';
import { useMessageAudioFocus } from '../media/message-audio-provider';
import { useTheme } from '../theme';
import { MessageMarkdown } from './message-markdown';

/** Read-only source content. The existing media renderer owns every preview lease. */
export function ForwardMessagePreview({
  spaceId,
  entry,
  isCurrent,
  contentWidth,
  options = DEFAULT_FORWARD_OPTIONS,
}: {
  spaceId: string;
  entry: TimelineEntry;
  isCurrent(): boolean;
  contentWidth?: number;
  options?: ForwardOptions;
}) {
  const { colors } = useTheme();
  const { width } = useWindowDimensions();
  useMessageAudioFocus(spaceId, `forward:${entry.id}`);
  const displayed = forwardPreviewEntry(entry, options);
  if (displayed.deleted || !('Message' in displayed.kind)) return null;
  const { card, text } = displayed.kind.Message;
  const media = Boolean(entry.voice || card?.kind_id === 'file');
  return (
    <View testID="forward-reviewed-content" style={{ gap: 8 }}>
      {media ? (
        <View
          style={{
            borderRadius: 24,
            overflow: 'hidden',
            backgroundColor: entry.voice && entry.author.is_me ? colors.mine : 'transparent',
          }}
        >
          <MessageMedia spaceId={spaceId} entry={displayed} isCurrent={isCurrent} />
        </View>
      ) : card ? (
        <View style={{ gap: 4 }}>
          <Text selectable style={{ color: colors.text, fontSize: 17 }}>
            {card.title}
          </Text>
          <Text selectable style={{ color: colors.secondary }}>
            {card.kind_label}
          </Text>
          {card.summary ? (
            <Text selectable style={{ color: colors.text }}>
              {card.summary}
            </Text>
          ) : null}
        </View>
      ) : null}
      {text ? (
        <MessageMarkdown
          text={text}
          color={colors.text}
          fontSize={17}
          maxWidth={contentWidth ?? Math.max(1, width - 40)}
          selectable
          testID="forward-reviewed-text"
        />
      ) : null}
    </View>
  );
}
