import { Stack, router } from 'expo-router';
import { ComposerAction } from '../../components/composer-action';
import { SkyScene } from '../../components/sky-background';
import { t } from '../../i18n';
import { useTheme } from '../../theme';

/** Catalog headers belong to this modal, never to the headerless tab navigator. */
export default function AppCatalogLayout() {
  const { colors } = useTheme();
  return (
    <SkyScene>
      <Stack
        screenOptions={{
          headerBackButtonDisplayMode: 'minimal',
          headerShadowVisible: false,
          headerTransparent: true,
          headerBlurEffect: 'none',
          headerTintColor: colors.text,
          headerTitleStyle: { color: colors.text },
          contentStyle: { backgroundColor: 'transparent' },
          scrollEdgeEffects: { top: 'hidden', bottom: 'hidden' },
        }}
      >
        <Stack.Screen
          name="index"
          options={{
            title: t('miniapps.miniapps'),
            headerLeft: () => (
              <ComposerAction
                testID="apps-catalog-close"
                name="xmark"
                label={t('common.close')}
                size={44}
                onPress={() => router.back()}
              />
            ),
          }}
        />
      </Stack>
    </SkyScene>
  );
}
