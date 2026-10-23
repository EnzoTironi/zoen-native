import { Column, Host, ListItem, Text } from '@expo/ui';
import { accessibilityHint, accessibilityLabel, frame, padding } from '@expo/ui/swift-ui/modifiers';
import { View } from 'react-native';
import { useTheme } from '../theme';
import { Icon } from './primitives';
import type { SettingsRow } from './settings-row-types';

export type { SettingsRow } from './settings-row-types';

/** Native rows without a second scrolling container or an opaque grouped-list background. */
export function SettingsRows({ rows, testID }: { rows: readonly SettingsRow[]; testID: string }) {
  const { presentation: p, scheme } = useTheme();
  return (
    <View
      testID={testID}
      style={{ width: '100%', backgroundColor: p.list.block, borderRadius: 10, overflow: 'hidden' }}
    >
      <Host
        matchContents={{ vertical: true }}
        style={{ width: '100%', backgroundColor: 'transparent' }}
        colorScheme={scheme}
        seedColor={p.list.accent}
      >
        <Column
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
              colors={{ containerColor: 'transparent', contentColor: p.list.text }}
              leading={
                <View
                  accessible={false}
                  accessibilityElementsHidden
                  style={{
                    width: 29,
                    height: 29,
                    borderRadius: 7,
                    backgroundColor: row.iconColor ?? 'transparent',
                    alignItems: 'center',
                    justifyContent: 'center',
                  }}
                >
                  {row.leading ??
                    (row.icon ? (
                      <Icon
                        name={row.icon}
                        size={20}
                        color={row.iconColor ? '#FFFFFF' : p.list.accent}
                      />
                    ) : null)}
                </View>
              }
              trailing={
                <View accessible={false} accessibilityElementsHidden>
                  <Icon
                    name={{ ios: 'chevron.right', android: 'chevron_right', web: 'chevron_right' }}
                    size={13}
                    color={p.list.disclosure}
                  />
                </View>
              }
              supportingText={
                row.supportingText ? (
                  <Text textStyle={{ color: p.list.secondary, fontSize: 13 }}>
                    {row.supportingText}
                  </Text>
                ) : undefined
              }
              modifiers={
                process.env.EXPO_OS === 'ios'
                  ? [
                      frame({
                        minHeight: row.supportingText ? 58 : 44,
                        maxWidth: Infinity,
                        alignment: 'leading',
                      }),
                      padding({ horizontal: 16 }),
                      accessibilityLabel(
                        row.supportingText ? `${row.title}, ${row.supportingText}` : row.title,
                      ),
                      ...(row.hint ? [accessibilityHint(row.hint)] : []),
                    ]
                  : undefined
              }
            >
              <Text textStyle={{ color: p.list.text, fontSize: 17 }}>{row.title}</Text>
            </ListItem>
          ))}
        </Column>
      </Host>
    </View>
  );
}
