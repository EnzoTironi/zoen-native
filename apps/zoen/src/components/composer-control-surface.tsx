import { GlassView, isLiquidGlassAvailable } from 'expo-glass-effect';
import type { ReactNode } from 'react';
import { Image } from 'expo-image';
import { View } from 'react-native';
import { composerGlassAppearance } from '../core/telegram-composer-presentation';
import { useTheme } from '../theme';
import { Icon } from './primitives';

/** Controls and field share the parent GlassContainer; no separate SwiftUI effect tree. */
export function ComposerControlSurface({
  children,
  size = 40,
}: {
  children: ReactNode;
  size?: number;
}) {
  const { scheme, presentation: p } = useTheme();
  const glass = process.env.EXPO_OS === 'ios' && isLiquidGlassAvailable();
  const appearance = composerGlassAppearance(scheme === 'dark', 'panel');
  const style = {
    width: size,
    height: size,
    borderRadius: size / 2,
    alignItems: 'center' as const,
    justifyContent: 'center' as const,
  };
  return glass ? (
    <GlassView
      pointerEvents="none"
      colorScheme={scheme}
      glassEffectStyle={appearance.style}
      tintColor={appearance.tintColor}
      isInteractive
      style={style}
    >
      {children}
    </GlassView>
  ) : (
    <View pointerEvents="none" style={[style, { backgroundColor: p.chat.input.fill }]}>
      {children}
    </View>
  );
}

export function ComposerGlyph({
  name,
  color,
}: {
  name:
    | 'plus'
    | 'mic'
    | 'arrow.up'
    | 'checkmark'
    | 'stop.fill'
    | 'xmark'
    | 'trash'
    | 'chevron.up'
    | 'chevron.down'
    | 'keyboard'
    | 'face.smiling';
  color: string;
}) {
  const glyphs = {
    plus: {
      source: require('../../assets/telegram-composer/attach@2x.png'),
      width: 30,
      height: 30,
    },
    mic: { source: require('../../assets/telegram-composer/mic@2x.png'), width: 30, height: 30 },
    'arrow.up': {
      source: require('../../assets/telegram-composer/send@2x.png'),
      width: 34,
      height: 34,
    },
    keyboard: {
      source: require('../../assets/telegram-composer/keyboard@2x.png'),
      width: 29,
      height: 24,
    },
    'face.smiling': {
      source: require('../../assets/telegram-composer/stickers@2x.png'),
      width: 24,
      height: 24,
    },
  };
  const glyph =
    process.env.EXPO_OS === 'ios' && name in glyphs
      ? glyphs[name as keyof typeof glyphs]
      : undefined;
  if (glyph)
    return (
      <Image
        source={glyph.source}
        contentFit="contain"
        tintColor={color}
        accessible={false}
        style={{ width: glyph.width, height: glyph.height }}
      />
    );
  const material = {
    plus: 'add',
    mic: 'mic',
    'arrow.up': 'arrow_upward',
    checkmark: 'check',
    'stop.fill': 'stop',
    xmark: 'close',
    trash: 'delete_outline',
    'chevron.up': 'expand_less',
    'chevron.down': 'expand_more',
    keyboard: 'keyboard',
    'face.smiling': 'sentiment_satisfied',
  } as const;
  return (
    <Icon
      name={{ ios: name, android: material[name], web: material[name] }}
      size={26}
      color={color}
    />
  );
}
