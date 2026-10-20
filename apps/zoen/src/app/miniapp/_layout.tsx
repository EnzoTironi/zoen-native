import { Stack } from 'expo-router';
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
        <Stack.Screen name="[id]" options={{ headerShown: false }} />
      </Stack>
    </SkyScene>
  );
}
