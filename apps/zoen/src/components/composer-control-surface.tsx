import type { ReactNode } from 'react';
import { Image } from 'expo-image';
import { ComposerGlassSurface } from './composer-glass';
import { Icon } from './primitives';

/** Controls and field share the original native Telegram glass container. */
export function ComposerControlSurface({
  children,
  size = 40,
}: {
  children: ReactNode;
  size?: number;
}) {
  const style = {
    width: size,
    height: size,
    borderRadius: size / 2,
    borderWidth: 0,
    alignItems: 'center' as const,
    justifyContent: 'center' as const,
  };
  return (
    <ComposerGlassSurface pointerEvents="none" cornerRadius={size / 2} style={style}>
      {children}
    </ComposerGlassSurface>
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
      source: require('../../assets/telegram-composer/attach.png'),
      width: 30,
      height: 30,
    },
    mic: { source: require('../../assets/telegram-composer/mic.png'), width: 30, height: 30 },
    'arrow.up': {
      source: require('../../assets/telegram-composer/send.png'),
      width: 34,
      height: 34,
    },
    keyboard: {
      source: require('../../assets/telegram-composer/keyboard.png'),
      width: 29,
      height: 24,
    },
    'face.smiling': {
      source: require('../../assets/telegram-composer/stickers.png'),
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
