import { Host } from '@expo/ui';
import { Button, HStack, List, Section, Text as NativeText } from '@expo/ui/swift-ui';
import {
  buttonStyle,
  contentShape,
  disabled,
  environment,
  font,
  foregroundStyle,
  frame,
  labelStyle,
  listRowBackground,
  listStyle,
  moveDisabled,
  scrollContentBackground,
  shapes,
} from '@expo/ui/swift-ui/modifiers';
import { useCallback, useLayoutEffect, useMemo, useRef, useState } from 'react';
import { ActivityIndicator, View } from 'react-native';
import type { ItemDetail } from '../core/generated/ItemDetail';
import { moveHomeOrder } from '../home/order';
import { t } from '../i18n';
import { useTheme } from '../theme';
import type { ConversationOrderListProps } from './conversation-order-list.types';

const keyExtractor = (item: ItemDetail) => item.id;

export function ConversationOrderList({
  items,
  pinnedIds,
  onReorder,
  onSetPinned,
}: ConversationOrderListProps) {
  const { colors, scheme } = useTheme();
  const [busy, setBusy] = useState(false);
  const [nativeReset, setNativeReset] = useState(0);
  const pending = useRef(false);
  const signature = JSON.stringify([items.map((item) => item.id), pinnedIds, nativeReset]);
  const live = useRef({ mounted: false, signature });
  useLayoutEffect(() => {
    live.current = { mounted: true, signature };
    return () => {
      live.current.mounted = false;
    };
  }, [signature]);
  const { pinned, hidden } = useMemo(() => {
    const byId = new Map(items.map((item) => [item.id, item]));
    const selected = new Set(pinnedIds);
    return {
      pinned: pinnedIds.flatMap((id) => {
        const item = byId.get(id);
        return item ? [item] : [];
      }),
      hidden: items.filter((item) => !selected.has(item.id)),
    };
  }, [items, pinnedIds]);
  const perform = useCallback(
    (action: () => Promise<void>) => {
      if (pending.current || !live.current.mounted || live.current.signature !== signature) return;
      pending.current = true;
      setBusy(true);
      void (async () => {
        try {
          await action();
        } catch {
          // The controller publishes save errors and retains the committed presentation.
          // SwiftUI moves rows before persistence settles. Remount its List on
          // failure so that its visual order returns to the committed props too.
          if (live.current.mounted) setNativeReset((revision) => revision + 1);
        } finally {
          pending.current = false;
          if (live.current.mounted) setBusy(false);
        }
      })();
    },
    [signature],
  );
  const move = useCallback(
    (sources: number[], destination: number) => {
      const ids = pinned.map((item) => item.id);
      const order = moveHomeOrder(ids, sources, destination);
      if (!order || order.every((id, index) => id === ids[index])) return;
      perform(() => onReorder(order));
    },
    [pinned, perform, onReorder],
  );
  const renderPinned = useCallback(
    ({ item }: { item: ItemDetail }) => (
      <HStack
        testID={`conversation-order-pinned-${item.id}`}
        spacing={12}
        modifiers={[
          frame({ minHeight: 48, alignment: 'leading' }),
          listRowBackground(colors.surface),
          moveDisabled(busy),
        ]}
      >
        <Button
          testID={`conversation-order-hide-${item.id}`}
          label={t('conversationCards.unpinNamed', { name: item.title })}
          systemImage="minus.circle.fill"
          onPress={() => perform(() => onSetPinned(item.id, false))}
          modifiers={[
            buttonStyle('borderless'),
            labelStyle('iconOnly'),
            frame({ minWidth: 44, minHeight: 44 }),
            contentShape(shapes.rectangle()),
            foregroundStyle(colors.danger),
            disabled(busy),
          ]}
        />
        <NativeText
          modifiers={[
            font({ textStyle: 'body' }),
            foregroundStyle(colors.text),
            frame({ maxWidth: Infinity, alignment: 'leading' }),
          ]}
        >
          {item.title}
        </NativeText>
      </HStack>
    ),
    [busy, colors.surface, colors.danger, colors.text, perform, onSetPinned],
  );
  const renderHidden = useCallback(
    ({ item }: { item: ItemDetail }) => (
      <HStack
        testID={`conversation-order-hidden-${item.id}`}
        spacing={12}
        modifiers={[
          frame({ minHeight: 48, alignment: 'leading' }),
          listRowBackground(colors.surface),
        ]}
      >
        <Button
          testID={`conversation-order-show-${item.id}`}
          label={t('conversationCards.pinNamed', { name: item.title })}
          systemImage="plus.circle.fill"
          onPress={() => perform(() => onSetPinned(item.id, true))}
          modifiers={[
            buttonStyle('borderless'),
            labelStyle('iconOnly'),
            frame({ minWidth: 44, minHeight: 44 }),
            contentShape(shapes.rectangle()),
            foregroundStyle(colors.accent),
            disabled(busy),
          ]}
        />
        <NativeText
          modifiers={[
            font({ textStyle: 'body' }),
            foregroundStyle(colors.text),
            frame({ maxWidth: Infinity, alignment: 'leading' }),
          ]}
        >
          {item.title}
        </NativeText>
      </HStack>
    ),
    [busy, colors.surface, colors.accent, colors.text, perform, onSetPinned],
  );
  return (
    <View style={{ flex: 1 }}>
      <Host
        testID="conversation-order-list-host"
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
          <Section title={t('conversationCards.pinned')}>
            {pinned.length ? (
              <List.ForEach
                key={signature}
                data={pinned}
                keyExtractor={keyExtractor}
                recycling={false}
                onMove={move}
              >
                {renderPinned}
              </List.ForEach>
            ) : (
              <NativeText
                modifiers={[foregroundStyle(colors.secondary), listRowBackground(colors.surface)]}
              >
                {t('conversationCards.noPinned')}
              </NativeText>
            )}
          </Section>
          <Section title={t('conversationCards.hidden')}>
            {hidden.length ? (
              <List.ForEach
                key={signature}
                data={hidden}
                keyExtractor={keyExtractor}
                recycling={false}
              >
                {renderHidden}
              </List.ForEach>
            ) : (
              <NativeText
                modifiers={[foregroundStyle(colors.secondary), listRowBackground(colors.surface)]}
              >
                {t('conversationCards.noHidden')}
              </NativeText>
            )}
          </Section>
        </List>
      </Host>
      {busy ? (
        <ActivityIndicator
          accessible
          accessibilityLabel={t('conversationCards.saving')}
          color={colors.accent}
          style={{ position: 'absolute', top: 8, right: 24 }}
        />
      ) : null}
    </View>
  );
}
