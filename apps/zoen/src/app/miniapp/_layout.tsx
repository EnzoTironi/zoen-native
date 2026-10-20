import { Stack } from 'expo-router';
import { SafeAreaProvider } from 'react-native-safe-area-context';
import { SkyScene } from '../../components/sky-background';
import { AppSheetGrabber } from '../../miniapps/app-sheet-grabber';
import { useTheme } from '../../theme';

export default function AppsLayout() {
  const { colors } = useTheme();
  const content = (
    <SkyScene>
      <Stack
        screenOptions={{
          orientation: 'default',
          title: '',
          headerTitle: '',
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
  return (
    <SafeAreaProvider>
      {process.env.EXPO_OS === 'web' ? content : <AppSheetGrabber>{content}</AppSheetGrabber>}
    </SafeAreaProvider>
  );
}
