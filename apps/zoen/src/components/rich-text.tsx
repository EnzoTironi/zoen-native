import { Linking, Text, type TextStyle } from 'react-native';
import type { TextSpanDto } from '../core/generated/TextSpanDto';
import { useTheme } from '../theme';

export function RichText({
  text,
  spans,
  style,
}: {
  text: string;
  spans: TextSpanDto[];
  style?: TextStyle;
}) {
  const { colors } = useTheme();
  const boundaries = [
    ...new Set([0, text.length, ...spans.flatMap((span) => [span.start, span.end])]),
  ]
    .filter((position) => position >= 0 && position <= text.length)
    .sort((a, b) => a - b);
  return (
    <Text selectable style={[{ color: colors.text, fontSize: 17, lineHeight: 27 }, style]}>
      {boundaries.slice(0, -1).map((start, index) => {
        const end = boundaries[index + 1]!;
        const marks = spans.filter((span) => span.start <= start && span.end >= end);
        const link = marks.find((span) => span.key === 'a')?.value;
        const safe = link && /^(https?:|mailto:|tel:|zoen:)/i.test(link) ? link : null;
        return (
          <Text
            key={`${start}:${end}`}
            accessibilityRole={safe ? 'link' : undefined}
            onPress={
              safe
                ? () => {
                    void Linking.openURL(safe).catch(() => {});
                  }
                : undefined
            }
            style={{
              fontWeight: marks.some((mark) => mark.key === 'b') ? '700' : undefined,
              fontStyle: marks.some((mark) => mark.key === 'i') ? 'italic' : undefined,
              textDecorationLine: marks.some((mark) => mark.key === 's')
                ? 'line-through'
                : safe
                  ? 'underline'
                  : undefined,
              fontFamily: marks.some((mark) => mark.key === 'c')
                ? process.env.EXPO_OS === 'ios'
                  ? 'Menlo'
                  : 'monospace'
                : undefined,
              color: safe ? colors.accent : undefined,
            }}
          >
            {text.slice(start, end)}
          </Text>
        );
      })}
    </Text>
  );
}
