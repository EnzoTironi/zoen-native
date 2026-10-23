/** UIKit softens scrolling content behind navigation without an opaque header band. */
export const navigationGlassAppearance = {
  headerTransparent: process.env.EXPO_OS !== 'android',
  headerBlurEffect: 'none' as const,
  headerStyle: { backgroundColor: 'transparent' },
  headerLargeStyle: { backgroundColor: 'transparent' },
  headerShadowVisible: false,
  scrollEdgeEffects: { top: 'automatic' as const, bottom: 'hidden' as const },
};
