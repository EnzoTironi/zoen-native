// SPDX-License-Identifier: GPL-2.0-or-later
// Telegram-iOS f1dd7a2 ThemeSettingsThemeItem: reveal the selected theme on entry
// and after selection. Scrolling to browse never changes the selected appearance.
import { useCallback, useLayoutEffect, useRef, type ReactNode } from 'react';
import { ScrollView, type ScrollViewInstance } from 'react-native';
import { useTheme } from '../theme';
import { THEME_PROFILES, type ThemeProfile } from '../theme/catalog';

const ITEM_WIDTH = 116;
const INSET = 6;
export function ThemePresetList({
  profile,
  children,
}: {
  profile: ThemeProfile;
  children: ReactNode;
}) {
  const { reducedMotion } = useTheme();
  const list = useRef<ScrollViewInstance>(null);
  const geometry = useRef({ width: 0, content: 0 });
  const selection = useRef<ThemeProfile | null>(null);
  const reveal = useCallback(
    (animated: boolean) => {
      const { width, content } = geometry.current;
      if (width <= 0 || content <= 0) return;
      const center = INSET + THEME_PROFILES.indexOf(profile) * ITEM_WIDTH + ITEM_WIDTH / 2;
      const x = Math.max(0, Math.min(content - width, center - width / 2));
      list.current?.scrollTo({ x, animated });
      selection.current = profile;
    },
    [profile],
  );
  useLayoutEffect(() => {
    reveal(selection.current !== null && !reducedMotion);
  }, [reveal, reducedMotion]);
  return (
    <ScrollView
      ref={list}
      testID="theme-preset-list"
      horizontal
      showsHorizontalScrollIndicator={false}
      contentContainerStyle={{ paddingHorizontal: INSET }}
      onLayout={(event) => {
        geometry.current.width = event.nativeEvent.layout.width;
        reveal(false);
      }}
      onContentSizeChange={(width) => {
        geometry.current.content = width;
        reveal(false);
      }}
    >
      {children}
    </ScrollView>
  );
}
