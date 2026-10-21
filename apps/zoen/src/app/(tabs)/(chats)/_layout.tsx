import { SkyScene } from '../../../components/sky-background';
import { Stack } from 'expo-router';
import { useTheme } from '../../../theme';
export default function ChatsLayout() {
  const { presentation: p } = useTheme();
  return (
    <Stack
      screenLayout={({ children }) => <SkyScene tabScene>{children}</SkyScene>}
      screenOptions={{
        headerShadowVisible: false,
        headerTransparent: process.env.EXPO_OS !== 'android',
        headerBlurEffect: 'none',
        headerStyle: { backgroundColor: 'transparent' },
        headerLargeStyle: { backgroundColor: 'transparent' },
        headerLargeTitleShadowVisible: false,
        headerTitleStyle: { ...p.typography.title, color: p.navigation.text },
        headerLargeTitleStyle: { ...p.typography.largeTitle, color: p.navigation.text },
        scrollEdgeEffects: { top: 'hidden', bottom: 'hidden' },
        headerTintColor: p.navigation.control,
        contentStyle: { backgroundColor: 'transparent' },
      }}
    />
  );
}
