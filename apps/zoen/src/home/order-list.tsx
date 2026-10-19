import { useLayoutEffect, useRef, useState } from 'react';
import { ActivityIndicator, FlatList, Text, View } from 'react-native';
import { NativeButton } from '../components/primitives';
import type { ItemDetail } from '../core/generated/ItemDetail';
import { t } from '../i18n';
import { useTheme } from '../theme';
import { moveHomeOrder } from './order';

export function HomeOrderList({
  items,
  onReorder,
}: {
  items: ItemDetail[];
  onReorder: (ids: readonly string[]) => Promise<void>;
}) {
  const { colors } = useTheme();
  const [busy, setBusy] = useState(false);
  const pending = useRef(false);
  const signature = JSON.stringify(items.map((item) => item.id));
  const live = useRef({ mounted: false, signature });
  useLayoutEffect(() => {
    live.current = { mounted: true, signature };
    return () => {
      live.current.mounted = false;
    };
  }, [signature]);
  function move(index: number, destination: number) {
    if (pending.current || !live.current.mounted || live.current.signature !== signature) return;
    const order = moveHomeOrder(
      items.map((item) => item.id),
      [index],
      destination,
    );
    if (!order) return;
    pending.current = true;
    setBusy(true);
    void (async () => {
      try {
        await onReorder(order);
      } catch {
        // HomeController publishes the failed save while retaining the committed order.
      } finally {
        pending.current = false;
        if (live.current.mounted) setBusy(false);
      }
    })();
  }
  return (
    <View style={{ flex: 1 }}>
      <FlatList
        data={items}
        keyExtractor={(item) => item.id}
        contentInsetAdjustmentBehavior="never"
        contentContainerStyle={{ padding: 24, gap: 12 }}
        renderItem={({ item, index }) => (
          <View
            testID={`home-order-${item.id}`}
            style={{ padding: 16, gap: 12, borderRadius: 16, backgroundColor: colors.surface }}
          >
            <Text selectable style={{ color: colors.text, fontSize: 17 }}>
              {item.title}
            </Text>
            {item.space_title ? (
              <Text style={{ color: colors.secondary }}>{item.space_title}</Text>
            ) : null}
            <View style={{ flexDirection: 'row', flexWrap: 'wrap', gap: 8 }}>
              <NativeButton
                secondary
                label={t('home.moveEarlier')}
                disabled={busy || index === 0}
                onPress={() => move(index, index - 1)}
              />
              <NativeButton
                secondary
                label={t('home.moveLater')}
                disabled={busy || index === items.length - 1}
                onPress={() => move(index, index + 2)}
              />
            </View>
          </View>
        )}
      />
      {busy ? (
        <ActivityIndicator
          accessible
          accessibilityLabel={t('home.saving')}
          color={colors.accent}
          style={{ position: 'absolute', top: 8, right: 24 }}
        />
      ) : null}
    </View>
  );
}
