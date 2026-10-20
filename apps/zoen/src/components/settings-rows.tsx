import { Column, Host, ListItem, Text } from '@expo/ui';
import { accessibilityHint, accessibilityLabel, frame, padding } from '@expo/ui/swift-ui/modifiers';
import type { ReactNode } from 'react';
import { View } from 'react-native';
import type { SymbolViewProps } from 'expo-symbols';
import { useTheme } from '../theme';
import { Icon } from './primitives';

export type SettingsRow = {
  id: string;
  title: string;
  hint?: string;
  icon?: SymbolViewProps['name'];
  leading?: ReactNode;
  supportingText?: string;
  onPress: () => void;
};

/** Native rows without a second scrolling container or an opaque grouped-list background. */
export function SettingsRows({ rows, testID }: { rows: readonly SettingsRow[]; testID: string }) {
  const { colors, scheme } = useTheme();
  return (
    <Host
      matchContents={{ vertical: true }}
      style={{ width: '100%', backgroundColor: 'transparent' }}
      colorScheme={scheme}
      seedColor={colors.accent}
    >
      <Column
        testID={testID}
        spacing={0}
        modifiers={
          process.env.EXPO_OS === 'ios'
            ? [frame({ maxWidth: Infinity, alignment: 'leading' })]
            : undefined
        }
      >
        {rows.map((row) => (
          <ListItem
            key={row.id}
            testID={row.id}
            onPress={row.onPress}
            colors={{ containerColor: 'transparent', contentColor: colors.text }}
            leading={
              <View
                accessible={false}
                accessibilityElementsHidden
                style={{
                  minWidth: 28,
                  minHeight: 44,
                  alignItems: 'center',
                  justifyContent: 'center',
                }}
              >
                {row.leading ?? (row.icon ? <Icon name={row.icon} size={22} /> : null)}
              </View>
            }
            trailing={
              <View accessible={false} accessibilityElementsHidden>
                <Icon
                  name={{ ios: 'chevron.right', android: 'chevron_right', web: 'chevron_right' }}
                  size={13}
                  color={colors.tertiary}
                />
              </View>
            }
            supportingText={
              row.supportingText ? (
                <Text textStyle={{ color: colors.secondary }}>{row.supportingText}</Text>
              ) : undefined
            }
            modifiers={
              process.env.EXPO_OS === 'ios'
                ? [
                    padding({ vertical: 6 }),
                    frame({ minHeight: 56, maxWidth: Infinity, alignment: 'leading' }),
                    accessibilityLabel(
                      row.supportingText ? `${row.title}, ${row.supportingText}` : row.title,
                    ),
                    ...(row.hint ? [accessibilityHint(row.hint)] : []),
                  ]
                : undefined
            }
          >
            <Text textStyle={{ color: colors.text }}>{row.title}</Text>
          </ListItem>
        ))}
      </Column>
    </Host>
  );
}
