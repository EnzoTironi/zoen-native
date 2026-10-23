import { GlassView, isLiquidGlassAvailable } from 'expo-glass-effect';
import type { ReactNode } from 'react';
import { View } from 'react-native';
import { useTheme } from '../theme';

/** One native material for the inbox and conversation navigation controls. */
export function NavigationGlass({
  children,
  radius = 24,
}: {
  children: ReactNode;
  radius?: number;
}) {
  const { presentation: p, scheme } = useTheme();
  return process.env.EXPO_OS === 'ios' && isLiquidGlassAvailable() ? (
    <GlassView
      isInteractive
      colorScheme={scheme}
      glassEffectStyle="regular"
      style={{ borderRadius: radius }}
    >
      {children}
    </GlassView>
  ) : (
    <View style={{ borderRadius: radius, backgroundColor: p.navigation.background }}>
      {children}
    </View>
  );
}
