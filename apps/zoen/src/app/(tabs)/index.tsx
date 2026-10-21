import { router } from 'expo-router';
import { useState } from 'react';
import Animated, { useAnimatedRef } from 'react-native-reanimated';
import { ActivityIndicator, Pressable, Text, View, useWindowDimensions } from 'react-native';
import { EmptyState, Icon, NativeButton } from '../../components/primitives';
import { SkyScene } from '../../components/sky-background';
import { HomeActions } from '../../home/menus';
import { useHome } from '../../home/provider';
import { t } from '../../i18n/index.ts';
import { ReorderableAppCards, useCardScrollPort } from '../../miniapps/reorderable-cards';
import { mergeCardSubset } from '../../miniapps/card-reorder';
import { appCardGrid } from '../../miniapps/app-card-layout';
import type { ItemDetail } from '../../core/generated/ItemDetail';
import { useTheme } from '../../theme';

export default function Home() {
  const home = useHome();
  const [editOwner, setEditOwner] = useState<string | null>(null);
  const editing = editOwner !== null && editOwner === home.ownerKey;
  const setEditing = (value: boolean) => setEditOwner(value ? home.ownerKey : null);
  const [pending, setPending] = useState({ pinned: false, available: false });
  const scrollPort = useCardScrollPort(false);
  const scrollRef = useAnimatedRef<Animated.ScrollView>();
  const { colors } = useTheme();
  const { width } = useWindowDimensions();
  const available = home.items.filter((item) => !home.isPinned(item.id));
  const saving =
    (pending.pinned && home.pinnedItems.length > 0) || (pending.available && available.length > 0);
  const contentWidth = Math.min(width, 720) - 40;
  const { columns } = appCardGrid(contentWidth);
  function grid(items: ItemDetail[], favorites: boolean) {
    const complete = home.items.map((item) => item.id);
    return (
      <ReorderableAppCards
        key={`${home.ownerKey}:${favorites ? 'pinned' : 'available'}`}
        items={items}
        contextKey={JSON.stringify(complete)}
        columns={columns}
        scrollPort={scrollPort}
        scrollRef={scrollRef}
        editing={editing}
        onEditingChange={setEditing}
        onPendingChange={(value) => {
          const group = favorites ? 'pinned' : 'available';
          setPending((previous) =>
            previous[group] === value ? previous : { ...previous, [group]: value },
          );
        }}
        onReorder={(next, expected) =>
          favorites
            ? home.reorder(next, expected)
            : home.reorderItems(mergeCardSubset(complete, next), complete)
        }
        onOpen={(item) => router.push({ pathname: '/miniapp/[id]', params: { id: item.id } })}
        onRemove={(item) => home.hide(item.id)}
        onTogglePin={(item) => (home.isPinned(item.id) ? home.unpin(item.id) : home.pin(item.id))}
        isPinned={home.isPinned}
      />
    );
  }
  return (
    <SkyScene tabScene>
      <Animated.ScrollView
        ref={scrollRef}
        onScroll={scrollPort.onScroll}
        scrollEventThrottle={16}
        onContentSizeChange={(_width, height) => scrollPort.extent.set(height)}
        contentInsetAdjustmentBehavior="automatic"
        contentContainerStyle={{
          flexGrow: 1,
          paddingHorizontal: 20,
          paddingTop: 18,
          paddingBottom: 36,
          gap: 28,
          maxWidth: 720,
          width: '100%',
          alignSelf: 'center',
        }}
      >
        <View
          style={{
            flexDirection: 'row',
            alignItems: 'center',
            justifyContent: 'space-between',
            gap: 16,
          }}
        >
          <Text
            testID="home-title"
            accessibilityRole="header"
            style={{ color: colors.text, fontSize: 34, fontWeight: '700', letterSpacing: -1 }}
          >
            {t('home.title')}
          </Text>
          {editing ? (
            <NativeButton
              testID="home-rearrange-done"
              label={saving ? t('home.saving') : t('home.doneRearranging')}
              disabled={saving}
              onPress={() => setEditing(false)}
            />
          ) : (
            <HomeActions
              canOrganize={home.pinnedItems.length > 1}
              hiddenCount={home.hiddenItems.length}
              onRestoreHidden={() => {
                void home.restoreAll().catch(() => {});
              }}
            />
          )}
        </View>
        {home.pendingRequestCount > 0 ? (
          <Pressable
            accessibilityRole="button"
            accessibilityLabel={`${t('home.pendingReviews', { count: home.pendingRequestCount })}. ${t('home.review')}`}
            onPress={() => router.push('/activity')}
            style={({ pressed }) => ({
              flexDirection: 'row',
              alignItems: 'center',
              gap: 14,
              paddingVertical: 12,
              opacity: pressed ? 0.6 : 1,
            })}
          >
            <Icon
              name={{ ios: 'checkmark.bubble', android: 'task_alt' }}
              size={26}
              color={colors.accent}
            />
            <View style={{ flex: 1, gap: 4 }}>
              <Text style={{ color: colors.text, fontSize: 17, fontWeight: '600' }}>
                {t('home.pendingReviews', { count: home.pendingRequestCount })}
              </Text>
              <Text style={{ color: colors.secondary, fontSize: 15 }}>{t('home.review')}</Text>
            </View>
            <Icon
              name={{ ios: 'chevron.right', android: 'chevron_right' }}
              size={15}
              color={colors.secondary}
            />
          </Pressable>
        ) : null}
        {home.error ? (
          <View accessibilityRole="alert" style={{ gap: 12 }}>
            <Text style={{ color: colors.danger }}>{home.error}</Text>
            <NativeButton label={t('home.tryAgain')} onPress={home.retry} />
          </View>
        ) : null}
        {home.loading && home.items.length === 0 ? (
          <View style={{ paddingVertical: 32, gap: 12, alignItems: 'center' }}>
            <ActivityIndicator color={colors.secondary} />
            <Text style={{ color: colors.secondary }}>{t('home.loading')}</Text>
          </View>
        ) : null}
        {home.pinnedItems.length > 0 ? (
          <View style={{ gap: 16 }}>
            <Text
              accessibilityRole="header"
              style={{ color: colors.text, fontSize: 22, fontWeight: '600' }}
            >
              {t('home.pinned')}
            </Text>
            {grid(home.pinnedItems, true)}
          </View>
        ) : null}
        {available.length > 0 ? (
          <View style={{ gap: 16 }}>
            <Text
              accessibilityRole="header"
              style={{ color: colors.text, fontSize: 22, fontWeight: '600' }}
            >
              {t('home.miniapps')}
            </Text>
            {grid(available, false)}
          </View>
        ) : null}
        {!home.loading && !home.error && home.items.length === 0 ? (
          <View style={{ gap: 16, paddingVertical: 32 }}>
            <EmptyState title={t('home.emptyTitle')} detail={t('home.emptyDetail')} />
            <NativeButton label={t('home.explore')} onPress={() => router.push('/miniapps')} />
          </View>
        ) : null}
      </Animated.ScrollView>
    </SkyScene>
  );
}
