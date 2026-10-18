import { Host } from '@expo/ui';
import { Button, HStack, List, Section, Text as NativeText, VStack } from '@expo/ui/swift-ui';
import {
  accessibilityAddTraits,
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
  padding,
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
const pinnedKeyExtractor = (id: string) => id;

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
  // Native drag revisions track order, not background refreshes of card content.
  const pinnedKey = JSON.stringify(pinnedIds);
  const pinned = useMemo(() => JSON.parse(pinnedKey) as string[], [pinnedKey]);
  const byId = useMemo(() => new Map(items.map((item) => [item.id, item])), [items]);
  const signature = JSON.stringify([items.map((item) => item.id), pinnedIds, nativeReset]);
  const live = useRef({ mounted: false, signature });
  useLayoutEffect(() => {
    live.current = { mounted: true, signature };
    console.warn('[CardsMoveQA] ready');
    return () => {
      live.current.mounted = false;
    };
  }, [signature]);
  const hidden = useMemo(() => {
    const selected = new Set(pinned);
    return items.filter((item) => !selected.has(item.id));
  }, [items, pinned]);
  const perform = useCallback(
    (action: () => Promise<void>) => {
      console.warn(
        '[CardsMoveQA] perform',
        pending.current,
        live.current.mounted,
        live.current.signature === signature,
      );
      // Keep native row editability stable across inserts; serialize saves here.
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
      console.warn(
        '[CardsMoveQA] move',
        sources.length,
        destination,
        pending.current,
        live.current.mounted,
        live.current.signature === signature,
      );
      const order = moveHomeOrder(pinned, sources, destination);
      if (!order || order.every((id, index) => id === pinned[index])) return;
      perform(() => onReorder(order));
    },
    [pinned, perform, onReorder, signature],
  );
  const renderPinned = useCallback(
    ({ item: id }: { item: string }) => {
      const item = byId.get(id);
      if (!item) return <NativeText />;
      return (
        <HStack
          testID={`conversation-order-pinned-${item.id}`}
          spacing={12}
          modifiers={[
            frame({ minHeight: 48, alignment: 'leading' }),
            listRowBackground(colors.surface),
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
      );
    },
    [byId, busy, colors.surface, colors.danger, colors.text, perform, onSetPinned],
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
        <VStack alignment="leading" spacing={0}>
          <NativeText
            modifiers={[
              font({ textStyle: 'subheadline', weight: 'medium' }),
              foregroundStyle(colors.secondary),
              padding({ horizontal: 16 }),
              accessibilityAddTraits(['isHeader']),
            ]}
          >
            {t('conversationCards.pinned')}
          </NativeText>
          <List
            key={nativeReset}
            modifiers={[
              listStyle('insetGrouped'),
              scrollContentBackground('hidden'),
              environment('editMode', 'active'),
            ]}
          >
            {pinned.length ? (
              <List.ForEach data={pinned} keyExtractor={pinnedKeyExtractor} onMove={move}>
                {renderPinned}
              </List.ForEach>
            ) : (
              <NativeText
                modifiers={[foregroundStyle(colors.secondary), listRowBackground(colors.surface)]}
              >
                {t('conversationCards.noPinned')}
              </NativeText>
            )}
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
        </VStack>
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
