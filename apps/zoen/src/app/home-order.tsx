import { Stack } from 'expo-router';
import { useHeaderHeight } from 'expo-router/react-navigation';
import { Text, View } from 'react-native';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import { EmptyState, Loading, NativeButton } from '../components/primitives';
import { SkyScene } from '../components/sky-background';
import { HomeOrderList } from '../home/order-list';
import { useHome } from '../home/provider';
import { t } from '../i18n';
import { useTheme } from '../theme';

export default function HomeOrder() {
  const home = useHome();
  const headerHeight = useHeaderHeight();
  const insets = useSafeAreaInsets();
  const { colors } = useTheme();
  return (
    <SkyScene>
      <Stack.Screen options={{ title: t('home.organize') }} />
      <View style={{ flex: 1, paddingTop: headerHeight, paddingBottom: insets.bottom }}>
        <Text style={{ color: colors.secondary, fontSize: 16, lineHeight: 23, padding: 24 }}>
          {t('home.organizeHint')}
        </Text>
        {home.error ? (
          <View style={{ paddingHorizontal: 24, paddingBottom: 16, gap: 12 }}>
            <Text selectable accessibilityRole="alert" style={{ color: colors.danger }}>
              {home.error}
            </Text>
            <NativeButton secondary label={t('home.tryAgain')} onPress={home.retry} />
          </View>
        ) : null}
        {home.pinnedItems.length > 0 ? (
          <HomeOrderList key={home.ownerKey} items={home.pinnedItems} onReorder={home.reorder} />
        ) : home.loading ? (
          <Loading />
        ) : !home.error ? (
          <EmptyState title={t('home.noPinned')} detail={t('home.noPinnedDetail')} />
        ) : null}
      </View>
    </SkyScene>
  );
}
