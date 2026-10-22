/** Keep the sky continuous; NavigationGlass alone supplies the controls' native material. */
export const navigationGlassAppearance = {
  headerTransparent: process.env.EXPO_OS !== 'android',
  headerBlurEffect: 'none' as const,
  headerStyle: { backgroundColor: 'transparent' },
  headerLargeStyle: { backgroundColor: 'transparent' },
  headerShadowVisible: false,
  scrollEdgeEffects: { top: 'hidden' as const, bottom: 'hidden' as const },
};
