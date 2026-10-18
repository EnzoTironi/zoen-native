import { SkyScene } from '../../../components/sky-background';
import { Stack } from 'expo-router';
import { useTheme } from '../../../theme';
export default function ChatsLayout() {
  const { colors } = useTheme();
  return (
    <Stack
      screenLayout={({ children }) => <SkyScene>{children}</SkyScene>}
      screenOptions={{
        headerShadowVisible: false,
        headerTransparent: true,
        headerBlurEffect: 'none',
        headerStyle: { backgroundColor: 'transparent' },
        headerLargeStyle: { backgroundColor: 'transparent' },
        headerLargeTitleShadowVisible: false,
        headerTitleStyle: { color: colors.text },
        headerLargeTitleStyle: { color: colors.text },
        scrollEdgeEffects: { top: 'hidden', bottom: 'hidden' },
        headerTintColor: colors.text,
        contentStyle: { backgroundColor: 'transparent' },
      }}
    />
  );
}
