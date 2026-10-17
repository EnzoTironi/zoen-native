import { ScrollView, Text, View } from 'react-native';
import type { ItemDetail } from '../core/generated/ItemDetail';
import { useTheme } from '../theme';
import { CompactMiniappCard } from './compact-card';

export function MiniappRail({
  items,
  label,
  size = 152,
  paddingVertical = 12,
  onLongPress,
}: {
  items: readonly ItemDetail[];
  label?: string;
  size?: number;
  paddingVertical?: number;
  onLongPress?: (item: ItemDetail) => void;
}) {
  const { colors } = useTheme();
  const miniapps = items.filter((item) => item.app !== null);
  if (!miniapps.length) return null;
  return (
    <View style={{ gap: 10, paddingVertical, backgroundColor: 'transparent' }}>
      {label ? (
        <Text
          accessibilityRole="header"
          style={{ color: colors.text, fontSize: 18, fontWeight: '600', paddingHorizontal: 20 }}
        >
          {label}
        </Text>
      ) : null}
      <ScrollView
        horizontal
        showsHorizontalScrollIndicator={false}
        contentInsetAdjustmentBehavior="never"
        contentContainerStyle={{ gap: 10, paddingHorizontal: 20, paddingBottom: 4 }}
      >
        {miniapps.map((item) => (
          <CompactMiniappCard
            key={item.id}
            item={item}
            size={size}
            onLongPress={onLongPress ? () => onLongPress(item) : undefined}
          />
        ))}
      </ScrollView>
    </View>
  );
}
