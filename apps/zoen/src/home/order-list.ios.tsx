import { Host } from '@expo/ui';
import { List, Text as NativeText, VStack } from '@expo/ui/swift-ui';
import {
  environment,
  font,
  foregroundStyle,
  frame,
  listRowBackground,
  listStyle,
  moveDisabled,
  scrollContentBackground,
} from '@expo/ui/swift-ui/modifiers';
import { useCallback, useLayoutEffect, useRef, useState } from 'react';
import { ActivityIndicator, View } from 'react-native';
import type { ItemDetail } from '../core/generated/ItemDetail';
import { t } from '../i18n';
import { useTheme } from '../theme';
import { moveHomeOrder } from './order';

const keyExtractor = (item: ItemDetail) => item.id;

export function HomeOrderList({
  items,
  onReorder,
}: {
  items: ItemDetail[];
  onReorder: (ids: readonly string[]) => Promise<void>;
}) {
  const { colors, scheme } = useTheme();
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
  const move = useCallback(
    (sources: number[], destination: number) => {
      if (pending.current || !live.current.mounted || live.current.signature !== signature) return;
      const ids = items.map((item) => item.id);
      const order = moveHomeOrder(ids, sources, destination);
      if (!order || order.every((id, index) => id === ids[index])) return;
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
    },
    [items, onReorder, signature],
  );
  const row = useCallback(
    ({ item }: { item: ItemDetail }) => (
      <VStack
        testID={`home-order-${item.id}`}
        alignment="leading"
        spacing={4}
        modifiers={[
          frame({ minHeight: 48, alignment: 'leading' }),
          listRowBackground(colors.surface),
          moveDisabled(busy),
        ]}
      >
        <NativeText modifiers={[font({ textStyle: 'body' }), foregroundStyle(colors.text)]}>
          {item.title}
        </NativeText>
        {item.space_title ? (
          <NativeText
            modifiers={[font({ textStyle: 'caption' }), foregroundStyle(colors.secondary)]}
          >
            {item.space_title}
          </NativeText>
        ) : null}
      </VStack>
    ),
    [busy, colors.surface, colors.text, colors.secondary],
  );
  return (
    <View style={{ flex: 1 }}>
      <Host
        testID="home-order-list-host"
        style={{ flex: 1 }}
        colorScheme={scheme}
        seedColor={colors.accent}
      >
        <List
          modifiers={[
            listStyle('insetGrouped'),
            scrollContentBackground('hidden'),
            environment('editMode', 'active'),
          ]}
        >
          <List.ForEach data={items} keyExtractor={keyExtractor} onMove={move}>
            {row}
          </List.ForEach>
        </List>
      </Host>
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
