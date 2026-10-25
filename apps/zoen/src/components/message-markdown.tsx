import { memo, useMemo } from 'react';
import { Text } from 'react-native';
import { EnrichedMarkdownText, type MarkdownStyle } from 'react-native-enriched-markdown';
import { messageMarkdownLink, messageMarkdownMayLoadMedia } from '../core/message-markdown';
import { useTheme } from '../theme';
import { useLinkLaunch } from './use-link-launch';

export type MessageMarkdownProps = {
  text: string;
  color: string;
  fontSize: number;
  maxWidth: number;
  testID?: string;
  selectable?: boolean;
};

/** Same native Markdown engine as the document editor, within the message face. */
export const MessageMarkdown = memo(function MessageMarkdown({
  text,
  color,
  fontSize,
  maxWidth,
  testID,
  selectable = false,
}: MessageMarkdownProps) {
  const { colors } = useTheme();
  const launchLink = useLinkLaunch(text);
  const markdownStyle = useMemo<MarkdownStyle>(() => {
    const body = { color, fontSize, lineHeight: Math.ceil(fontSize * 1.3) };
    const heading = (extra: number) => ({
      color,
      fontSize: fontSize + extra,
      fontWeight: '700',
      marginTop: 8,
      marginBottom: 4,
    });
    const mono = process.env.EXPO_OS === 'ios' ? 'Menlo' : 'monospace';
    return {
      paragraph: { ...body, marginTop: 0, marginBottom: 6 },
      h1: heading(6),
      h2: heading(4),
      h3: heading(2),
      h4: heading(0),
      h5: heading(0),
      h6: heading(0),
      strong: { color },
      em: { color },
      link: { color: colors.accent, underline: false },
      list: { ...body, marginTop: 3, marginBottom: 6, gapWidth: 6, itemSpacing: 2 },
      blockquote: {
        ...body,
        borderColor: colors.accent,
        borderWidth: 3,
        backgroundColor: colors.surface,
        borderRadius: 4,
        padding: 6,
        marginTop: 4,
        marginBottom: 6,
      },
      code: { color, fontFamily: mono, backgroundColor: colors.surface },
      codeBlock: {
        ...body,
        fontFamily: mono,
        fontSize: Math.max(13, fontSize - 2),
        backgroundColor: colors.surface,
        borderColor: colors.line,
        borderWidth: 0.5,
        borderRadius: 6,
        padding: 8,
        marginTop: 4,
        marginBottom: 6,
      },
      table: {
        ...body,
        fontSize: Math.max(13, fontSize - 1),
        headerTextColor: color,
        headerBackgroundColor: colors.surface,
        borderColor: colors.line,
        borderWidth: 0.5,
        borderRadius: 4,
        cellPaddingHorizontal: 8,
        cellPaddingVertical: 6,
        marginTop: 4,
        marginBottom: 6,
      },
      thematicBreak: { color: colors.line, height: 0.5, marginTop: 6, marginBottom: 6 },
      taskList: { checkedColor: colors.accent, borderColor: colors.secondary },
      spoiler: { color: colors.secondary },
    };
  }, [color, colors, fontSize]);
  if (process.env.EXPO_OS !== 'ios' && messageMarkdownMayLoadMedia(text))
    return (
      <Text
        testID={testID}
        selectable={selectable}
        allowFontScaling
        style={{ color, fontSize, lineHeight: Math.ceil(fontSize * 1.3), maxWidth, flexShrink: 1 }}
      >
        {text}
      </Text>
    );
  return (
    <EnrichedMarkdownText
      testID={testID}
      markdown={text}
      flavor="github"
      markdownStyle={markdownStyle}
      containerStyle={{ minWidth: 0, maxWidth, flexShrink: 1 }}
      md4cFlags={{
        hardSoftBreaks: true,
        preserveBlankLines: true,
        latexMath: false,
        noMedia: process.env.EXPO_OS === 'ios',
      }}
      selectable={selectable}
      allowFontScaling
      enableLinkPreview={false}
      enableBlockContextMenu={selectable}
      enableTaskListItemToggle={false}
      streamingAnimation={false}
      onLinkPress={({ url }) => {
        const target = messageMarkdownLink(url);
        if (target) void launchLink(target);
      }}
    />
  );
});
