import { Stack, router } from 'expo-router';
import { t } from '../../i18n/index.ts';
import { IconButton } from '../../components/primitives';
import { SkyScene } from '../../components/sky-background';
import { useTheme } from '../../theme';

export default function AppsLayout() {
  const { colors } = useTheme();
  return (
    <SkyScene>
      <Stack
        screenOptions={{
          title: '',
          headerBackButtonDisplayMode: 'minimal',
          headerShadowVisible: false,
          headerTransparent: false,
          headerBlurEffect: 'none',
          headerStyle: { backgroundColor: 'transparent' },
          headerTitleStyle: { color: colors.text },
          headerTintColor: colors.text,
          contentStyle: { backgroundColor: 'transparent' },
          scrollEdgeEffects: { top: 'hidden', bottom: 'hidden' },
        }}
      >
        <Stack.Screen
          name="[id]"
          options={{
            headerBackVisible: false,
            headerLeft: () => (
              <IconButton
                name={{ ios: 'xmark', android: 'close', web: 'close' }}
                label={t('common.close')}
                onPress={() => {
                  if (router.canGoBack()) router.back();
                  else router.replace('/');
                }}
              />
            ),
          }}
        />
      </Stack>
    </SkyScene>
  );
}
