import type { ReactNode } from 'react';
import { Pressable, ScrollView, Text, View } from 'react-native';
import { useTheme } from '../theme';
export function SearchFilterRow({
  options,
  selected,
  onSelect,
  horizontal = false,
  trailing,
}: {
  options: string[];
  selected: number;
  onSelect(index: number): void;
  horizontal?: boolean;
  trailing?: ReactNode;
}) {
  const { colors } = useTheme();
  const row = (
    <View
      style={{
        flexDirection: 'row',
        flexWrap: horizontal ? 'nowrap' : 'wrap',
        gap: 8,
        marginHorizontal: 20,
      }}
    >
      {options.map((label, index) => (
        <Pressable
          key={label}
          accessibilityLabel={label}
          accessibilityRole="button"
          accessibilityState={{ selected: selected === index }}
          onPress={() => onSelect(index)}
          style={{
            minHeight: 44,
            justifyContent: 'center',
            paddingHorizontal: 14,
            borderRadius: 22,
            backgroundColor: selected === index ? colors.muted : 'transparent',
          }}
        >
          <Text style={{ color: colors.text }}>{label}</Text>
        </Pressable>
      ))}
      {trailing}
    </View>
  );
  return horizontal ? (
    <ScrollView
      horizontal
      style={{ flexGrow: 0, flexShrink: 0, minHeight: 44 }}
      contentInsetAdjustmentBehavior="never"
      showsHorizontalScrollIndicator={false}
    >
      {row}
    </ScrollView>
  ) : (
    row
  );
}
