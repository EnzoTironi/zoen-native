import { t } from '../i18n/index.ts';
import { router, usePathname } from 'expo-router';
import { Pressable, Text } from 'react-native';
import { Icon } from './primitives';
import { NativeTabs } from 'expo-router/native-tabs';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import { useTheme } from '../theme';
import { skyGradient } from './sky-background';
export default function AppNavigation() {
  const { colors, sky, scheme } = useTheme();
  const pathname = usePathname();
  const insets = useSafeAreaInsets();
  const contentStyle = {
    // NativeTabs handles Android's bottom inset; the headerless tab screens own the top.
    ...(process.env.EXPO_OS === 'android' ? { paddingTop: insets.top } : {}),
    ...(process.env.EXPO_OS === 'android'
      ? { backgroundColor: 'transparent' }
      : {
          backgroundColor: sky.bottom,
          experimental_backgroundImage: skyGradient(sky),
        }),
  };
  return (
    <NativeTabs
      // Removing accessory children leaves UIKit's plate installed; hide the host itself.
      unstable_nativeProps={{
        ios: { bottomAccessoryHidden: pathname !== '/' },
        ...(process.env.EXPO_OS === 'android'
          ? { colorScheme: scheme, nativeContainerStyle: { backgroundColor: 'transparent' } }
          : {}),
      }}
      {...(process.env.EXPO_OS === 'android'
        ? {
            backgroundColor: 'transparent',
            indicatorColor: colors.muted,
            rippleColor: colors.accent,
          }
        : {})}
      tintColor={colors.accent}
      iconColor={{ default: colors.secondary, selected: colors.accent }}
      labelStyle={{
        default: { color: colors.secondary },
        selected: { color: colors.accent },
      }}
    >
      <NativeTabs.BottomAccessory>
        <HomeSearch />
      </NativeTabs.BottomAccessory>
      <NativeTabs.Trigger name="index" contentStyle={contentStyle}>
        <NativeTabs.Trigger.Label>{t('home.title')}</NativeTabs.Trigger.Label>
        <NativeTabs.Trigger.Icon sf={{ default: 'house', selected: 'house.fill' }} md="home" />
      </NativeTabs.Trigger>
      <NativeTabs.Trigger name="(chats)" contentStyle={contentStyle}>
        <NativeTabs.Trigger.Label>{t('common.chats')}</NativeTabs.Trigger.Label>
        <NativeTabs.Trigger.Icon sf="bubble.left.and.bubble.right" md="chat" />
      </NativeTabs.Trigger>
      <NativeTabs.Trigger name="files" contentStyle={contentStyle}>
        <NativeTabs.Trigger.Label>{t('common.files')}</NativeTabs.Trigger.Label>
        <NativeTabs.Trigger.Icon sf="folder" md="folder" />
      </NativeTabs.Trigger>
      <NativeTabs.Trigger name="you" contentStyle={contentStyle}>
        <NativeTabs.Trigger.Label>{t('common.you')}</NativeTabs.Trigger.Label>
        <NativeTabs.Trigger.Icon sf="person.crop.circle" md="account_circle" />
      </NativeTabs.Trigger>
    </NativeTabs>
  );
}

function HomeSearch() {
  const { colors } = useTheme();
  const placement = NativeTabs.BottomAccessory.usePlacement();
  return (
    <Pressable
      accessibilityRole="button"
      accessibilityLabel={t('home.search')}
      onPress={() => router.push('/search')}
      style={{
        flexDirection: 'row',
        alignItems: 'center',
        justifyContent: 'center',
        gap: 10,
        paddingHorizontal: 18,
        minHeight: 48,
      }}
    >
      <Icon name={{ ios: 'magnifyingglass', android: 'search' }} color={colors.text} size={20} />
      {placement === 'regular' ? (
        <Text style={{ color: colors.text, fontSize: 17 }}>{t('home.search')}</Text>
      ) : null}
    </Pressable>
  );
}
