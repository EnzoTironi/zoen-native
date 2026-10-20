import { Pressable, Text, View } from 'react-native';
import { useTheme } from '../theme';
import { Icon } from './primitives';
import type { SettingsRow } from './settings-row-types';

export type { SettingsRow } from './settings-row-types';

/** Keep browser settings outside the SwiftUI module boundary. */
export function SettingsRows({ rows, testID }: { rows: readonly SettingsRow[]; testID: string }) {
  const { colors } = useTheme();
  return (
    <View testID={testID} style={{ width: '100%' }}>
      {rows.map((row) => (
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
            minHeight: 56,
            paddingVertical: 6,
            flexDirection: 'row',
            alignItems: 'center',
            gap: 16,
            backgroundColor: 'transparent',
            opacity: pressed ? 0.5 : 1,
          })}
        >
          <View
            accessible={false}
            aria-hidden
            style={{
              minWidth: 28,
              minHeight: 44,
              flexShrink: 0,
              alignItems: 'center',
              justifyContent: 'center',
            }}
          >
            {row.leading ?? (row.icon ? <Icon name={row.icon} size={22} /> : null)}
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
