import { Host } from '@expo/ui';
import { List, Text as NativeText, VStack } from '@expo/ui/swift-ui';
import {
  environment,
  font,
  foregroundStyle,
  frame,
  listRowBackground,
  listStyle,
  scrollContentBackground,
} from '@expo/ui/swift-ui/modifiers';
import { useCallback, useLayoutEffect, useMemo, useRef, useState } from 'react';
import { ActivityIndicator, View } from 'react-native';
import type { ItemDetail } from '../core/generated/ItemDetail';
import { t } from '../i18n';
import { useTheme } from '../theme';
import { moveHomeOrder } from './order';

const keyExtractor = (id: string) => id;

export function HomeOrderList({
  items,
  onReorder,
}: {
  items: ItemDetail[];
  onReorder: (ids: readonly string[]) => Promise<void>;
}) {
  const { colors, scheme } = useTheme();
  const [busy, setBusy] = useState(false);
  const [nativeReset, setNativeReset] = useState(0);
  const pending = useRef(false);
  // Native drag revisions track order, not background refreshes of card content.
  const idsKey = JSON.stringify(items.map((item) => item.id));
  const ids = useMemo(() => JSON.parse(idsKey) as string[], [idsKey]);
  const byId = useMemo(() => new Map(items.map((item) => [item.id, item])), [items]);
  const signature = JSON.stringify([ids, nativeReset]);
  const live = useRef({ mounted: false, signature });
  useLayoutEffect(() => {
    live.current = { mounted: true, signature };
    return () => {
      live.current.mounted = false;
    };
  }, [signature]);
  const move = useCallback(
    (sources: number[], destination: number) => {
      // Keep native row editability stable across inserts; serialize saves here.
      if (pending.current || !live.current.mounted || live.current.signature !== signature) return;
      const order = moveHomeOrder(ids, sources, destination);
      if (!order || order.every((id, index) => id === ids[index])) return;
      pending.current = true;
      setBusy(true);
      void (async () => {
        try {
          await onReorder(order);
        } catch {
          // HomeController publishes the failed save while retaining the committed order.
          if (live.current.mounted) setNativeReset((revision) => revision + 1);
        } finally {
          pending.current = false;
          if (live.current.mounted) setBusy(false);
        }
      })();
    },
    [ids, onReorder, signature],
  );
  const row = useCallback(
    ({ item: id }: { item: string }) => {
      const item = byId.get(id);
      if (!item) return <NativeText />;
      return (
        <VStack
          testID={`home-order-${item.id}`}
          alignment="leading"
          spacing={4}
          modifiers={[
            frame({ minHeight: 48, alignment: 'leading' }),
            listRowBackground(colors.surface),
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
      );
    },
    [byId, colors.surface, colors.text, colors.secondary],
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
          key={nativeReset}
          modifiers={[
            listStyle('insetGrouped'),
            scrollContentBackground('hidden'),
            environment('editMode', 'active'),
          ]}
        >
          <List.ForEach key={signature} data={ids} keyExtractor={keyExtractor} onMove={move}>
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
