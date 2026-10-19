import { useLayoutEffect, useMemo, useRef, useState } from 'react';
import { ActivityIndicator, SectionList, Text, View } from 'react-native';
import { NativeButton } from '../components/primitives';
import { moveHomeOrder } from '../home/order';
import { t } from '../i18n';
import { useTheme } from '../theme';
import type { ConversationOrderListProps } from './conversation-order-list.types';

export function ConversationOrderList({
  items,
  pinnedIds,
  onReorder,
  onSetPinned,
}: ConversationOrderListProps) {
  const { colors } = useTheme();
  const [busy, setBusy] = useState(false);
  const pending = useRef(false);
  const signature = JSON.stringify([items.map((item) => item.id), pinnedIds]);
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
  function perform(action: () => Promise<void>) {
    if (pending.current || !live.current.mounted || live.current.signature !== signature) return;
    pending.current = true;
    setBusy(true);
    void (async () => {
      try {
        await action();
      } catch {
        // The controller publishes save errors and retains the committed presentation.
      } finally {
        pending.current = false;
        if (live.current.mounted) setBusy(false);
      }
    })();
  }
  function move(index: number, destination: number) {
    const ids = pinned.map((item) => item.id);
    const order = moveHomeOrder(ids, [index], destination);
    if (!order || order.every((id, position) => id === ids[position])) return;
    perform(() => onReorder(order));
  }
  return (
    <View style={{ flex: 1 }}>
      <SectionList
        testID="conversation-order-list"
        sections={[
          { key: 'pinned', title: t('conversationCards.pinned'), data: pinned },
          { key: 'hidden', title: t('conversationCards.hidden'), data: hidden },
        ]}
        keyExtractor={(item) => item.id}
        stickySectionHeadersEnabled={false}
        contentInsetAdjustmentBehavior="never"
        contentContainerStyle={{ padding: 24, gap: 12 }}
        renderSectionHeader={({ section }) => (
          <Text style={{ color: colors.text, fontSize: 20, fontWeight: '600' }}>
            {section.title}
          </Text>
        )}
        renderSectionFooter={({ section }) =>
          section.data.length ? null : (
            <Text style={{ color: colors.secondary, fontSize: 17 }}>
              {t(
                section.key === 'pinned'
                  ? 'conversationCards.noPinned'
                  : 'conversationCards.noHidden',
              )}
            </Text>
          )
        }
        renderItem={({ item, index, section }) => {
          const isPinned = section.key === 'pinned';
          return (
            <View
              testID={`conversation-order-${section.key}-${item.id}`}
              style={{ padding: 16, gap: 12, borderRadius: 16, backgroundColor: colors.surface }}
            >
              <Text selectable style={{ color: colors.text, fontSize: 17 }}>
                {item.title}
              </Text>
              <View style={{ flexDirection: 'row', flexWrap: 'wrap', gap: 8 }}>
                <NativeButton
                  secondary
                  testID={`conversation-order-${isPinned ? 'hide' : 'show'}-${item.id}`}
                  label={t(
                    isPinned ? 'conversationCards.unpinNamed' : 'conversationCards.pinNamed',
                    {
                      name: item.title,
                    },
                  )}
                  disabled={busy}
                  onPress={() => perform(() => onSetPinned(item.id, !isPinned))}
                />
                {isPinned ? (
                  <>
                    <NativeButton
                      secondary
                      label={t('home.moveEarlier')}
                      disabled={busy || index === 0}
                      onPress={() => move(index, index - 1)}
                    />
                    <NativeButton
                      secondary
                      label={t('home.moveLater')}
                      disabled={busy || index === pinned.length - 1}
                      onPress={() => move(index, index + 2)}
                    />
                  </>
                ) : null}
              </View>
            </View>
          );
        }}
      />
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
