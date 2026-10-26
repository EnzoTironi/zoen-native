import { memo, useMemo } from 'react';
import { Text, type LayoutChangeEvent } from 'react-native';
import { EnrichedMarkdownText, type MarkdownStyle } from 'react-native-enriched-markdown';
import { messageMarkdownLink, messageMarkdownMayLoadMedia } from '../core/message-markdown';
import { useTheme } from '../theme';
import { useLinkLaunch } from './use-link-launch';
import { useMessageCodeCopy } from './use-message-code-copy';

export type MessageMarkdownProps = {
  text: string;
  color: string;
  fontSize: number;
  maxWidth: number;
  testID?: string;
  selectable?: boolean;
  onLayout?: (event: LayoutChangeEvent) => void;
};

/** Same native Markdown engine as the document editor, within the message face. */
export const MessageMarkdown = memo(function MessageMarkdown({
  text,
  color,
  fontSize,
  maxWidth,
  testID,
  selectable = false,
  onLayout,
}: MessageMarkdownProps) {
  const { colors, scheme } = useTheme();
  const launchLink = useLinkLaunch(text);
  const copyCode = useMessageCodeCopy(text, testID);
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
      // Emphasis inherits its block ink, including the table's separate surface.
      link: { color: colors.accent, underline: false },
      list: { ...body, marginTop: 3, marginBottom: 6, gapWidth: 6, itemSpacing: 2 },
      blockquote: {
        ...body,
        color: colors.text,
        borderColor: colors.accent,
        borderWidth: 3,
        backgroundColor: colors.surface,
        borderRadius: 4,
        padding: 6,
        marginTop: 4,
        marginBottom: 6,
      },
      code: { color: colors.text, fontFamily: mono, backgroundColor: colors.surface },
      codeBlock: {
        ...body,
        color: colors.text,
        fontFamily: mono,
        fontSize: Math.max(13, fontSize - 2),
        backgroundColor: colors.surface,
        borderColor: colors.line,
        borderWidth: 0.5,
        borderRadius: 6,
        padding: 8,
        marginTop: 4,
        marginBottom: 6,
        syntaxColors: {
          comment: colors.secondary,
          ...(scheme === 'light'
            ? {
                keyword: colors.danger,
                string: colors.accent,
                number: colors.accent,
                constant: colors.accent,
                function: colors.accent,
                type: colors.danger,
                property: colors.accent,
                tag: colors.accent,
                attribute: colors.danger,
              }
            : {}),
        },
      },
      table: {
        ...body,
        color: colors.text,
        fontSize: Math.max(13, fontSize - 1),
        headerTextColor: colors.text,
        headerBackgroundColor: colors.surface,
        rowEvenBackgroundColor: colors.surface,
        rowOddBackgroundColor: colors.muted,
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
  }, [color, colors, fontSize, scheme]);
  if (process.env.EXPO_OS !== 'ios' && messageMarkdownMayLoadMedia(text))
    return (
      <Text
        testID={testID}
        onLayout={onLayout}
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
      onLayout={onLayout}
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
      onCodeBlockPress={({ code }) => {
        void copyCode(code);
      }}
      onLinkPress={({ url }) => {
        const target = messageMarkdownLink(url);
        if (target) void launchLink(target);
      }}
    />
  );
});
