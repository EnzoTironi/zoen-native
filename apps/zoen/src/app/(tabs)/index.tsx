import { router } from 'expo-router';
import {
  ActivityIndicator,
  Pressable,
  ScrollView,
  Text,
  View,
  useWindowDimensions,
} from 'react-native';
import { EmptyState, Icon, NativeButton } from '../../components/primitives';
import { SkyScene } from '../../components/sky-background';
import { HomeActions, HomeCardMenu } from '../../home/menus';
import { useHome } from '../../home/provider';
import { t } from '../../i18n/index.ts';
import { CompactMiniappCard } from '../../miniapps/compact-card';
import type { ItemDetail } from '../../core/generated/ItemDetail';
import { useTheme } from '../../theme';

export default function Home() {
  const home = useHome();
  const { colors } = useTheme();
  const { width, fontScale } = useWindowDimensions();
  const available = home.items.filter((item) => !home.isPinned(item.id));
  const contentWidth = Math.min(width, 720) - 40;
  const columns = fontScale > 1.5 || contentWidth < 300 ? 1 : contentWidth >= 600 ? 3 : 2;
  const cardSize = Math.max(136, (contentWidth - (columns - 1) * 14) / columns);
  function movePinned(id: string, direction: -1 | 1) {
    const ids = home.pinnedItems.map((item) => item.id);
    const index = ids.indexOf(id);
    const next = index + direction;
    if (index < 0 || next < 0 || next >= ids.length) return;
    [ids[index], ids[next]] = [ids[next]!, ids[index]!];
    void home.reorder(ids).catch(() => {});
  }
  function grid(items: ItemDetail[]) {
    return (
      <View style={{ flexDirection: 'row', flexWrap: 'wrap', gap: 14 }}>
        {items.map((item) => {
          const pinnedIndex = home.pinnedItems.findIndex((pin) => pin.id === item.id);
          return (
            <CompactMiniappCard
              key={item.id}
              item={item}
              size={cardSize}
              accessory={
                <HomeCardMenu
                  testID={`home-card-menu-${item.id}`}
                  name={item.title}
                  pinned={home.isPinned(item.id)}
                  onToggle={() => {
                    void (home.isPinned(item.id) ? home.unpin(item.id) : home.pin(item.id)).catch(
                      () => {},
                    );
                  }}
                  onMoveEarlier={pinnedIndex > 0 ? () => movePinned(item.id, -1) : undefined}
                  onMoveLater={
                    pinnedIndex >= 0 && pinnedIndex < home.pinnedItems.length - 1
                      ? () => movePinned(item.id, 1)
                      : undefined
                  }
                />
              }
            />
          );
        })}
      </View>
    );
  }
  return (
    <SkyScene tabScene>
      <ScrollView
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
            accessibilityRole="header"
            style={{ color: colors.text, fontSize: 34, fontWeight: '700', letterSpacing: -1 }}
          >
            {t('home.title')}
          </Text>
          <HomeActions canOrganize={home.pinnedItems.length > 1} />
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
            {grid(home.pinnedItems)}
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
            {grid(available)}
          </View>
        ) : null}
        {!home.loading && !home.error && home.items.length === 0 ? (
          <View style={{ gap: 16, paddingVertical: 32 }}>
            <EmptyState title={t('home.emptyTitle')} detail={t('home.emptyDetail')} />
            <NativeButton label={t('home.explore')} onPress={() => router.push('/miniapps')} />
          </View>
        ) : null}
      </ScrollView>
    </SkyScene>
  );
}
