import { SkyScene } from '../../../components/sky-background';
import { Stack } from 'expo-router';
import { useTheme } from '../../../theme';
import { navigationGlassAppearance } from '../../../components/navigation-appearance';
export default function ChatsLayout() {
  const { presentation: p } = useTheme();
  return (
    <Stack
      screenLayout={({ children }) => <SkyScene tabScene>{children}</SkyScene>}
      screenOptions={{
        ...navigationGlassAppearance,
        headerLargeTitleShadowVisible: false,
        headerTitleStyle: { ...p.typography.title, color: p.navigation.text },
        headerLargeTitleStyle: { ...p.typography.largeTitle, color: p.navigation.text },
        headerTintColor: p.navigation.control,
        contentStyle: { backgroundColor: 'transparent' },
      }}
    />
  );
}
