import { Text, View } from 'react-native';
import { router } from 'expo-router';
import { useState } from 'react';
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
  onRemove,
  onTogglePin,
  isPinned,
  restoreLabel,
  onRestore,
  error,
  onReorder,
}: {
  items: readonly ItemDetail[];
  label?: string;
  paddingVertical?: number;
  onLongPress?: (item: ItemDetail) => void;
  onRemove?: (item: ItemDetail) => Promise<unknown>;
  onTogglePin?: (item: ItemDetail) => Promise<unknown>;
  isPinned?: (id: string) => boolean;
  restoreLabel?: string;
  onRestore?: () => void;
  error?: string | null;
  onReorder?: (next: readonly string[], expected: readonly string[]) => Promise<unknown>;
}) {
  const { colors } = useTheme();
  const [editing, setEditing] = useState(false);
  const [pending, setPending] = useState(false);
  const scrollPort = useCardScrollPort(true);
  const scrollRef = useAnimatedRef<Animated.ScrollView>();
  const miniapps = items.filter((item) => item.app !== null);
  const saving = pending && miniapps.length > 0;
  if (!miniapps.length && !onRestore && !error) return null;
  return (
    <View style={{ gap: 10, paddingVertical, backgroundColor: 'transparent' }}>
      {error ? (
        <Text accessibilityRole="alert" style={{ color: colors.danger, paddingHorizontal: 20 }}>
          {error}
        </Text>
      ) : null}
      {onRestore ? (
        <View style={{ paddingHorizontal: 20, alignItems: 'flex-start' }}>
          <NativeButton
            testID="app-rail-restore"
            label={restoreLabel ?? t('home.restoreHidden')}
            secondary
            onPress={onRestore}
          />
        </View>
      ) : null}
      {label ? (
        <Text
          accessibilityRole="header"
          style={{ color: colors.text, fontSize: 18, fontWeight: '600', paddingHorizontal: 20 }}
        >
          {label}
        </Text>
      ) : null}
      {miniapps.length ? (
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
              onRemove={onRemove}
              onTogglePin={onTogglePin}
              isPinned={isPinned}
            />
          ) : (
            miniapps.map((item) => (
              <CompactMiniappCard
                key={item.id}
                item={item}
                onLongPress={onLongPress ? () => onLongPress(item) : undefined}
              />
            ))
          )}
        </Animated.ScrollView>
      ) : null}
      {editing ? (
        <View style={{ paddingHorizontal: 20, alignItems: 'flex-end', minHeight: 44 }}>
          <NativeButton
            testID="app-rail-rearrange-done"
            label={saving ? t('home.saving') : t('home.doneRearranging')}
            disabled={saving}
            onPress={() => setEditing(false)}
          />
        </View>
      ) : null}
    </View>
  );
}
