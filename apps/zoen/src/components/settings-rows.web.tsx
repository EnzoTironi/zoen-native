import { Pressable, Text, View } from 'react-native';
import { useTheme } from '../theme';
import { Icon } from './primitives';
import type { SettingsRow } from './settings-row-types';

export type { SettingsRow } from './settings-row-types';

/** Keep browser settings outside the SwiftUI module boundary. */
export function SettingsRows({ rows, testID }: { rows: readonly SettingsRow[]; testID: string }) {
  const { colors, presentation: p } = useTheme();
  return (
    <View
      testID={testID}
      style={{ width: '100%', backgroundColor: p.list.block, borderRadius: 10, overflow: 'hidden' }}
    >
      {rows.map((row, index) => (
        <Pressable
          key={row.id}
          testID={row.id}
          accessibilityRole="button"
          accessibilityLabel={
            row.supportingText ? `${row.title}, ${row.supportingText}` : row.title
          }
          accessibilityHint={row.hint}
          onPress={row.onPress}
          style={({ pressed }) => ({
            width: '100%',
            minHeight: row.supportingText ? 58 : 44,
            paddingHorizontal: 16,
            borderTopWidth: index ? 0.5 : 0,
            borderTopColor: p.list.separator,
            paddingVertical: 6,
            flexDirection: 'row',
            alignItems: 'center',
            gap: 12,
            backgroundColor: 'transparent',
            opacity: pressed ? 0.5 : 1,
          })}
        >
          <View
            accessible={false}
            aria-hidden
            style={{
              width: 29,
              height: 29,
              borderRadius: 7,
              backgroundColor: row.iconColor ?? 'transparent',
              flexShrink: 0,
              alignItems: 'center',
              justifyContent: 'center',
            }}
          >
            {row.leading ??
              (row.icon ? (
                <Icon name={row.icon} size={20} color={row.iconColor ? '#FFFFFF' : p.list.accent} />
              ) : null)}
          </View>
          <View style={{ flex: 1, minWidth: 0, gap: 4 }}>
            <Text style={{ color: colors.text, fontSize: 17 }}>{row.title}</Text>
            {row.supportingText ? (
              <Text style={{ color: colors.secondary, fontSize: 15, lineHeight: 22 }}>
                {row.supportingText}
              </Text>
            ) : null}
          </View>
          <View accessible={false} aria-hidden style={{ flexShrink: 0 }}>
            <Icon
              name={{ ios: 'chevron.right', android: 'chevron_right', web: 'chevron_right' }}
              size={13}
              color={colors.tertiary}
            />
          </View>
        </Pressable>
      ))}
    </View>
  );
}
