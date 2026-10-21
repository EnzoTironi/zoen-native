import { useCallback, useRef, useState } from 'react';
import { Stack, router, useFocusEffect, useLocalSearchParams } from 'expo-router';
import { FlashList, type FlashListRef } from '@shopify/flash-list';
import { Linking, Pressable, ScrollView, Text, View } from 'react-native';
import { useClient } from '../../core/provider';
import { useCoreQuery } from '../../core/query';
import type { ItemDetail } from '../../core/types';
import { EmptyState, Icon, Loading, NativeButton } from '../../components/primitives';
import { useTheme } from '../../theme';
import { formatLocale, t } from '../../i18n';
import { formatFileSize } from '../../media/file-size';
import { SharedMediaTile } from '../../profile/shared-media-tile';
import {
  sharedItems,
  sharedLinks,
  sharedMediaTab,
  sharedMediaTabs,
  type SharedLink,
  type SharedMediaTab,
} from '../../profile/shared-media-projection';
import { peerInfo } from '../../profile/telegram-profile-presentation';

export default function SharedMedia() {
  const { id, tab } = useLocalSearchParams<{ id: string; tab?: string }>();
  const { store, snapshot, mode } = useClient();
  const owner = `${mode}:${snapshot?.account?.identity_id ?? ''}:${snapshot?.account?.device_id ?? ''}:${store.sessionEpoch}:${id}`;
  return <SharedMediaContent key={owner} id={id} initial={sharedMediaTab(tab)} owner={owner} />;
}
function SharedMediaContent({
  id,
  initial,
  owner,
}: {
  id: string;
  initial: SharedMediaTab;
  owner: string;
}) {
  const { store, snapshot, timelines } = useClient();
  const { presentation: p } = useTheme();
  const items = useCoreQuery({ op: 'items' });
  const [tab, setTab] = useState(initial);
  const [loadingOlder, setLoadingOlder] = useState(false);
  const [failure, setFailure] = useState<string | null>(null);
  const loading = useRef(false);
  const offsets = useRef<Partial<Record<SharedMediaTab, number>>>({});
  const list = useRef<FlashListRef<ItemDetail | SharedLink>>(null);
  const active = useRef(false);
  const currentOwner = useCallback(() => {
    const next = store.getSnapshot();
    return (
      active.current &&
      `${next.mode}:${next.snapshot?.account?.identity_id ?? ''}:${next.snapshot?.account?.device_id ?? ''}:${store.sessionEpoch}:${id}` ===
        owner &&
      Boolean(next.snapshot?.conversations.some((chat) => chat.id === id))
    );
  }, [store, id, owner]);
  useFocusEffect(
    useCallback(() => {
      active.current = true;
      setLoadingOlder(loading.current);
      if (tab === 'links')
        void store
          .loadTimeline(id)
          .then(() => {
            if (active.current && currentOwner()) setFailure(null);
          })
          .catch((error: unknown) => {
            if (active.current && currentOwner())
              setFailure(error instanceof Error ? error.message : t('peerInfo.unavailable'));
          });
      return () => {
        active.current = false;
      };
    }, [store, id, tab, currentOwner]),
  );
  const chat = snapshot?.conversations.find((chat) => chat.id === id);
  const history = timelines[id];
  const links = sharedLinks(history?.entries ?? []);
  const data: (ItemDetail | SharedLink)[] =
    tab === 'links' ? links : sharedItems(items.data ?? [], id, tab);
  async function loadHistory(older = false) {
    if (loading.current || !active.current || !currentOwner()) return;
    loading.current = true;
    setLoadingOlder(true);
    setFailure(null);
    try {
      await store.loadTimeline(id, older);
    } catch (error) {
      if (active.current && currentOwner())
        setFailure(error instanceof Error ? error.message : t('peerInfo.unavailable'));
    } finally {
      loading.current = false;
      if (active.current && currentOwner()) setLoadingOlder(false);
    }
  }
  if (!chat)
    return (
      <EmptyState
        title={t('peerInfo.unavailable')}
        detail={t('chat.thisChatIsNoLongerOnThisDevice')}
      />
    );
  return (
    <View testID="shared-media-screen" style={{ flex: 1 }}>
      <Stack.Screen options={{ title: t('peerInfo.mediaTitle') }} />
      <FlashList
        key={tab}
        ref={list}
        data={data}
        keyExtractor={(item) => item.id}
        numColumns={tab === 'media' ? 3 : 1}
        contentInsetAdjustmentBehavior="automatic"
        contentContainerStyle={{ paddingBottom: 32 }}
        scrollEventThrottle={16}
        onScroll={(event) => {
          offsets.current[tab] = event.nativeEvent.contentOffset.y;
        }}
        onLoad={() => {
          list.current?.scrollToOffset({ offset: offsets.current[tab] ?? 0, animated: false });
        }}
        ListHeaderComponent={
          <View style={{ gap: 12, paddingBottom: 12 }}>
            <ScrollView
              horizontal
              showsHorizontalScrollIndicator={false}
              contentContainerStyle={{
                paddingHorizontal: peerInfo.edge,
                paddingVertical: 10,
                gap: 4,
              }}
            >
              {sharedMediaTabs.map((key) => (
                <Pressable
                  key={key}
                  testID={`shared-media-tab-${key}`}
                  accessibilityRole="tab"
                  accessibilityState={{ selected: key === tab }}
                  onPress={() => {
                    if (key !== tab) setTab(key);
                  }}
                  style={({ pressed }) => ({
                    minHeight: 44,
                    minWidth: 60,
                    height: peerInfo.tabs + 4,
                    paddingHorizontal: 16,
                    borderRadius: 20,
                    alignItems: 'center',
                    justifyContent: 'center',
                    backgroundColor: key === tab ? p.list.block : 'transparent',
                    opacity: pressed ? 0.6 : 1,
                  })}
                >
                  <Text
                    style={{
                      color: key === tab ? p.list.accent : p.list.secondary,
                      fontSize: 15,
                      fontWeight: '500',
                    }}
                  >
                    {t(`peerInfo.${key}`)}
                  </Text>
                </Pressable>
              ))}
            </ScrollView>
            {tab !== 'links' && items.data ? (
              <Text style={{ paddingHorizontal: 16, color: p.list.secondary, fontSize: 13 }}>
                {t('peerInfo.sharedCount', { count: data.length })}
              </Text>
            ) : null}
            {items.error || failure ? (
              <View style={{ gap: 8, paddingHorizontal: 16 }}>
                <Text accessibilityRole="alert" style={{ color: p.destructive }}>
                  {items.error ?? failure}
                </Text>
                <NativeButton
                  secondary
                  label={t('peerInfo.retry')}
                  onPress={() => {
                    items.retry();
                    if (tab === 'links') void loadHistory();
                  }}
                />
              </View>
            ) : null}
          </View>
        }
        ListEmptyComponent={
          (!items.data && !items.error && tab !== 'links') ||
          (!history && !failure && tab === 'links') ? (
            <Loading />
          ) : (
            <EmptyState title={t('peerInfo.empty')} detail={t('peerInfo.emptyDetail')} />
          )
        }
        ListFooterComponent={
          tab === 'links' && history?.has_more ? (
            <View style={{ padding: 16 }}>
              <NativeButton
                secondary
                disabled={loadingOlder}
                label={t(loadingOlder ? 'peerInfo.loadingOlder' : 'peerInfo.loadOlder')}
                onPress={() => {
                  void loadHistory(true);
                }}
              />
            </View>
          ) : null
        }
        renderItem={({ item }) =>
          tab === 'media' && 'kind_id' in item ? (
            <SharedMediaTile item={item} owner={owner} isCurrent={currentOwner} />
          ) : 'url' in item ? (
            <View>
              <Pressable
                accessibilityRole="link"
                accessibilityLabel={item.url}
                onPress={() => {
                  if (currentOwner()) void Linking.openURL(item.url).catch(() => {});
                }}
                style={({ pressed }) => ({
                  padding: 16,
                  minHeight: 72,
                  gap: 4,
                  backgroundColor: pressed ? p.list.highlighted : 'transparent',
                })}
              >
                <Text style={{ color: p.list.text, fontSize: 17, fontWeight: '500' }}>
                  {item.host}
                </Text>
                <Text numberOfLines={2} style={{ color: p.list.accent, fontSize: 15 }}>
                  {item.url}
                </Text>
              </Pressable>
              <Pressable
                accessibilityRole="button"
                accessibilityLabel={t('peerInfo.backToChat')}
                onPress={() => {
                  if (currentOwner())
                    router.dismissTo({
                      pathname: '/chat/[id]',
                      params: { id, jump: item.messageId },
                    });
                }}
                style={{ minHeight: 44, justifyContent: 'center' }}
              >
                <Text style={{ color: p.list.secondary, fontSize: 13 }}>
                  {t('peerInfo.backToChat')}
                </Text>
              </Pressable>
            </View>
          ) : (
            <Pressable
              accessibilityRole="button"
              accessibilityLabel={t('peerInfo.openItem', { name: item.title })}
              onPress={() => {
                if (currentOwner())
                  router.push({ pathname: '/item/[id]', params: { id: item.id } });
              }}
              style={({ pressed }) => ({
                minHeight: 72,
                flexDirection: 'row',
                alignItems: 'center',
                gap: 14,
                paddingHorizontal: 16,
                paddingVertical: 10,
                backgroundColor: pressed ? p.list.highlighted : 'transparent',
              })}
            >
              <Icon
                name={
                  item.app
                    ? { ios: 'square.grid.2x2', android: 'apps', web: 'apps' }
                    : item.file?.mime.startsWith('audio/')
                      ? { ios: 'waveform', android: 'graphic_eq', web: 'graphic_eq' }
                      : { ios: 'doc.fill', android: 'description', web: 'description' }
                }
                size={30}
                color={p.list.accent}
              />
              <View style={{ flex: 1, gap: 4 }}>
                <Text numberOfLines={2} style={{ color: p.list.text, fontSize: 17 }}>
                  {item.file?.name ?? item.title}
                </Text>
                <Text numberOfLines={1} style={{ color: p.list.secondary, fontSize: 13 }}>
                  {item.file ? formatFileSize(item.file.bytes, formatLocale) : item.kind_label}
                </Text>
              </View>
            </Pressable>
          )
        }
      />
    </View>
  );
}
