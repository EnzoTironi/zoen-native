import { Text, View } from 'react-native';
import { router } from 'expo-router';
import { useState, type ReactNode } from 'react';
import Animated, { useAnimatedRef } from 'react-native-reanimated';
import type { ItemDetail } from '../core/generated/ItemDetail';
import { NativeButton } from '../components/primitives';
import { t } from '../i18n';
import { useTheme } from '../theme';
import { CompactMiniappCard } from './compact-card';
import { APP_CARD_METRICS } from './app-card-layout';
import { ReorderableAppCards, useCardScrollPort } from './reorderable-cards';

export function MiniappRail({
  items,
  label,
  paddingVertical = 12,
  onLongPress,
  renderAccessory,
  onReorder,
}: {
  items: readonly ItemDetail[];
  label?: string;
  paddingVertical?: number;
  onLongPress?: (item: ItemDetail) => void;
  renderAccessory?: (item: ItemDetail) => ReactNode;
  onReorder?: (next: readonly string[], expected: readonly string[]) => Promise<unknown>;
}) {
  const { colors } = useTheme();
  const [editing, setEditing] = useState(false);
  const [pending, setPending] = useState(false);
  const scrollPort = useCardScrollPort(true);
  const scrollRef = useAnimatedRef<Animated.ScrollView>();
  const miniapps = items.filter((item) => item.app !== null);
  if (!miniapps.length) return null;
  return (
    <View style={{ gap: 10, paddingVertical, backgroundColor: 'transparent' }}>
      {label || editing ? (
        <View
          style={{
            paddingHorizontal: 20,
            flexDirection: 'row',
            alignItems: 'center',
            justifyContent: 'space-between',
            minHeight: 44,
          }}
        >
          <Text
            accessibilityRole="header"
            style={{ color: colors.text, fontSize: 18, fontWeight: '600' }}
          >
            {label ?? t('home.rearrange')}
          </Text>
          {editing ? (
            <NativeButton
              testID="app-rail-rearrange-done"
              label={pending ? t('home.saving') : t('home.doneRearranging')}
              disabled={pending}
              onPress={() => setEditing(false)}
            />
          ) : null}
        </View>
      ) : null}
      <Animated.ScrollView
        ref={scrollRef}
        onScroll={scrollPort.onScroll}
        scrollEventThrottle={16}
        onContentSizeChange={(width) => scrollPort.extent.set(width)}
        horizontal
        showsHorizontalScrollIndicator={false}
        contentInsetAdjustmentBehavior="never"
        contentContainerStyle={{
          gap: APP_CARD_METRICS.gap,
          paddingHorizontal: 20,
          paddingBottom: 4,
        }}
      >
        {onReorder ? (
          <ReorderableAppCards
            items={miniapps}
            columns={miniapps.length}
            horizontal
            scrollPort={scrollPort}
            scrollRef={scrollRef}
            editing={editing}
            onEditingChange={setEditing}
            onPendingChange={setPending}
            onReorder={onReorder}
            onOpen={(item) => router.push({ pathname: '/miniapp/[id]', params: { id: item.id } })}
            renderAccessory={renderAccessory}
          />
        ) : (
          miniapps.map((item) => (
            <CompactMiniappCard
              key={item.id}
              item={item}
              onLongPress={onLongPress ? () => onLongPress(item) : undefined}
              accessory={renderAccessory?.(item)}
            />
          ))
        )}
      </Animated.ScrollView>
    </View>
  );
}
