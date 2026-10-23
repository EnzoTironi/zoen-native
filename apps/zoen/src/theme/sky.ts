import {
  DEFAULT_THEME_CUSTOMIZATION,
  telegramPresentation,
  type ThemeCustomization,
} from './telegram.ts';

import { isThemeProfile, isDarkTheme, type ThemeProfile } from './catalog.ts';

/** Local civil time only: no location, solar service, network, or system appearance. */
export const SKY_PHASES = ['day', 'night'] as const;
export type SkyPhase = (typeof SKY_PHASES)[number];
export type ThemePreference = 'auto' | ThemeProfile;
export type SkyTextureWeights = { day: number; night: number };
export type SkyColors = {
  top: string;
  middle: string;
  bottom: string;
  stars: number;
  haze: number;
  transitionMs: number;
  textureWeights: SkyTextureWeights;
  textureOpacity: number;
};
export type SemanticColors = {
  background: string;
  surface: string;
  raised: string;
  muted: string;
  line: string;
  text: string;
  secondary: string;
  tertiary: string;
  accent: string;
  onAccent: string;
  mine: string;
  onMine: string;
  theirs: string;
  danger: string;
  read: string;
};
const anchors: ReadonlyArray<{
  phase: SkyPhase;
  minute: number;
  top: string;
  middle: string;
  bottom: string;
  stars: number;
  haze: number;
}> = [
  {
    phase: 'night',
    minute: 0,
    top: '#080F19',
    middle: '#0D1723',
    bottom: '#14212C',
    stars: 0.35,
    haze: 0.03,
  },
  {
    phase: 'day',
    minute: 360,
    top: '#ECF4FA',
    middle: '#F4F8FB',
    bottom: '#FBFCFC',
    stars: 0,
    haze: 0.05,
  },
  {
    phase: 'night',
    minute: 1200,
    top: '#080F19',
    middle: '#0D1723',
    bottom: '#14212C',
    stars: 0.35,
    haze: 0.03,
  },
];
const clamp = (value: number) => Math.min(1, Math.max(0, value));
const rgb = (hex: string) => [1, 3, 5].map((offset) => parseInt(hex.slice(offset, offset + 2), 16));
export function mixColor(from: string, to: string, progress: number): string {
  const a = rgb(from);
  const b = rgb(to);
  return `#${a
    .map((value, index) =>
      Math.round(value + (b[index]! - value) * clamp(progress))
        .toString(16)
        .padStart(2, '0'),
    )
    .join('')}`;
}
function luminance(color: string): number {
  const values = rgb(color).map((value) => {
    const channel = value / 255;
    return channel <= 0.04045 ? channel / 12.92 : ((channel + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * values[0]! + 0.7152 * values[1]! + 0.0722 * values[2]!;
}
export function contrastRatio(a: string, b: string): number {
  const x = luminance(a);
  const y = luminance(b);
  return (Math.max(x, y) + 0.05) / (Math.min(x, y) + 0.05);
}
function inkFor(background: string): string {
  const candidates = ['#181D24', '#F7FAFF', '#000000', '#FFFFFF'];
  return (
    candidates.find((color) => contrastRatio(color, background) >= 4.7) ??
    candidates.reduce((best, color) =>
      contrastRatio(color, background) > contrastRatio(best, background) ? color : best,
    )
  );
}
/** Move only as far as necessary toward a known contrasting color. */
function readable(candidate: string, backgrounds: string[], target: string, minimum = 4.5): string {
  for (let step = 0; step <= 100; step++) {
    const color = mixColor(candidate, target, step / 100);
    if (backgrounds.every((background) => contrastRatio(color, background) >= minimum))
      return color;
  }
  return target;
}
/** Same conservative haze endpoint as SkyBackground's radial glow. */
export function skyBackdrops(sky: SkyColors): string[] {
  const result: string[] = [];
  for (const [from, to] of [
    [sky.top, sky.middle],
    [sky.middle, sky.bottom],
  ]) {
    for (let position = 0; position <= 8; position++) {
      const color = mixColor(from!, to!, position / 8);
      // A photo may contain ANY pixel. Bound both black and white over the
      // actual sky before the decorative white haze, not the photo's average.
      for (const photo of [
        color,
        mixColor(color, '#000000', sky.textureOpacity),
        mixColor(color, '#FFFFFF', sky.textureOpacity),
      ]) {
        result.push(photo, mixColor(photo, '#FFFFFF', 0.04 * sky.haze));
      }
    }
  }
  return result;
}
function skyInk(sky: SkyColors): string {
  const backgrounds = skyBackdrops(sky);
  const score = (ink: string) =>
    Math.min(...backgrounds.map((background) => contrastRatio(ink, background)));
  return score('#181D24') >= score('#FFFFFF') ? '#181D24' : '#FFFFFF';
}
function constrainSky(sky: SkyColors, preferredInk?: string): { sky: SkyColors; text: string } {
  const text = preferredInk ?? skyInk(sky);
  const opposite = text === '#FFFFFF' ? '#000000' : '#FFFFFF';
  // Preserve curated hue and saturation wherever possible. Only a mixed dawn /
  // dusk frontier needs its outlying stop nudged into the same legibility band.
  for (let step = 0; step <= 100; step++) {
    const candidate = {
      ...sky,
      top: mixColor(sky.top, opposite, step / 100),
      middle: mixColor(sky.middle, opposite, step / 100),
      bottom: mixColor(sky.bottom, opposite, step / 100),
    };
    if (skyBackdrops(candidate).every((background) => contrastRatio(text, background) >= 4.8))
      return { sky: candidate, text };
  }
  return { sky: { ...sky, top: opposite, middle: opposite, bottom: opposite }, text };
}
function semanticColors(
  sky: SkyColors,
  text: string,
  presentation: ReturnType<typeof telegramPresentation>,
): { scheme: 'light' | 'dark'; colors: SemanticColors } {
  const scheme = text === '#FFFFFF' ? 'dark' : 'light';
  const dark = scheme === 'dark';
  // Content plates remain coherent. Mixing light and dark surface colors with
  // the clock produces muddy gray cards during otherwise light afternoons.
  const builtin = presentation.profile.startsWith('telegram-');
  const background = builtin ? presentation.list.background : dark ? '#000000' : '#F8FAFB';
  const surface = builtin ? `${presentation.list.block}F5` : dark ? '#1C1C1DF5' : '#FFFFFFF5';
  const raised = builtin ? `${presentation.list.modal}F5` : dark ? '#2C2C2EF5' : '#FFFFFFF5';
  const muted = builtin ? presentation.list.highlighted : dark ? '#272728' : '#EDEDF1';
  const theirs = presentation.chat.incoming.fill;
  const plates = [
    background,
    muted,
    theirs,
    ...skyBackdrops(sky),
    ...[surface, raised].flatMap((color) =>
      ['#000000', '#FFFFFF'].map((backdrop) => mixColor(backdrop, color, 245 / 255)),
    ),
  ];
  const foreground = (night: string, day: string) =>
    readable(dark ? night : day, plates, text, 4.6);
  const accent = readable(presentation.selectedAccent, plates, text, 4.6);
  const mine = presentation.chat.outgoing.fill;
  return {
    scheme,
    colors: {
      background,
      surface,
      raised,
      muted,
      theirs,
      text,
      line: dark ? '#2C3845' : '#DCE3E8',
      secondary: foreground('#B0BAC6', '#515B67'),
      tertiary: foreground('#A2ADBA', '#5C6671'),
      accent,
      onAccent: inkFor(accent),
      mine,
      onMine: presentation.chat.outgoing.text,
      danger: foreground('#FF9B9D', '#AE2843'),
      read: foreground('#8CCEE9', '#225C88'),
    },
  };
}
/** Keep native images decorative: every possible pixel must preserve the same
 * text contrast as the bare sky. A small margin covers channel quantization. */
function textureOpacityFor(sky: SkyColors, inks: string[], cap: number, minimum = 4.55): number {
  const safe = (textureOpacity: number) =>
    skyBackdrops({ ...sky, textureOpacity }).every((background) =>
      inks.every((ink) => contrastRatio(ink, background) >= minimum),
    );
  if (safe(cap)) return cap;
  let low = 0;
  let high = cap;
  for (let iteration = 0; iteration < 12; iteration++) {
    const opacity = (low + high) / 2;
    if (safe(opacity)) low = opacity;
    else high = opacity;
  }
  return Math.floor(low * 10_000) / 10_000;
}
export function normalizeAppearance(value: string | null): ThemePreference {
  if (['light', 'dawn', 'morning', 'sunset'].includes(value ?? '')) return 'day';
  if (value === 'dark' || value === 'deepNight') return 'night';
  return value === 'auto' || isThemeProfile(value) ? (value as ThemePreference) : 'auto';
}
export function resolveSkyTheme(
  preference: ThemePreference,
  date: Date,
  customization: ThemeCustomization = DEFAULT_THEME_CUSTOMIZATION,
) {
  const minute =
    date.getHours() * 60 +
    date.getMinutes() +
    date.getSeconds() / 60 +
    date.getMilliseconds() / 60_000;
  if (!Number.isFinite(minute)) throw new RangeError('A valid local clock is required.');
  const builtin = preference !== 'auto' && preference.startsWith('telegram-');
  const selectedPhase = preference === 'auto' ? 'auto' : isDarkTheme(preference) ? 'night' : 'day';
  const index =
    selectedPhase === 'auto'
      ? anchors.reduce((found, anchor, position) => (minute >= anchor.minute ? position : found), 0)
      : anchors.findIndex((anchor) => anchor.phase === selectedPhase);
  const from = anchors[Math.max(0, index)]!;
  const profile: ThemeProfile = preference === 'auto' ? from.phase : preference;
  const wallpaper =
    profile === 'telegram-day'
      ? ['#FFFFFF', '#FFFFFF']
      : profile === 'telegram-classic'
        ? ['#DBDDBB', '#88B884']
        : profile === 'telegram-tinted'
          ? ['#1B2836', '#121A22']
          : ['#598BF6', '#F38B58'];
  const top = customization.backgroundTop ?? (builtin ? wallpaper[0]! : from.top);
  const bottom = customization.backgroundBottom ?? (builtin ? wallpaper[1]! : from.bottom);
  // Only two shipped skies. Automatic mode selects Day06:00–20:00 and Night
  // outside it; it never synthesizes extra dawn/sunset themes or a gray blend.
  const rawSky: SkyColors = {
    top,
    middle:
      builtin || customization.backgroundTop || customization.backgroundBottom
        ? mixColor(top, bottom, 0.54)
        : from.middle,
    bottom,
    stars: builtin ? 0 : from.stars,
    haze: builtin ? 0 : from.haze,
    transitionMs: 0,
    textureWeights: {
      day: !builtin && from.phase === 'day' ? 1 : 0,
      night: !builtin && from.phase === 'night' ? 1 : 0,
    },
    textureOpacity: 0,
  };
  const { sky, text } = constrainSky(rawSky, from.phase === 'night' ? '#FFFFFF' : '#000000');
  // Preserve visible photography, then choose ink against that full envelope.
  // Only the primary label's immutable scheme limits opacity during twilight;
  // secondary and colored labels adapt toward that same readable primary ink.
  const cap = builtin ? 0 : 0.28 - sky.textureWeights.night * 0.08;
  const photoSky = { ...sky, textureOpacity: textureOpacityFor(sky, [text], cap, 4.7) };
  const basePresentation = telegramPresentation(profile, customization);
  const semantic = semanticColors(photoSky, text, basePresentation);
  const presentation = customization.accent
    ? telegramPresentation(profile, customization, semantic.colors.accent)
    : basePresentation;
  const inks = [
    semantic.colors.text,
    semantic.colors.secondary,
    semantic.colors.tertiary,
    semantic.colors.accent,
    semantic.colors.danger,
    semantic.colors.read,
  ];
  return {
    phase: from.phase,
    profile,
    sky: {
      ...photoSky,
      textureOpacity: textureOpacityFor(photoSky, inks, photoSky.textureOpacity),
    },
    ...semantic,
    presentation,
  };
}

/** The sky animates independently of React text. New ink must also read on the
 * outgoing layer; otherwise snap instead of briefly showing unreadable content. */
export function canTransitionSky(
  from: ReturnType<typeof resolveSkyTheme>,
  to: ReturnType<typeof resolveSkyTheme>,
): boolean {
  if (from.scheme !== to.scheme) return false;
  const oldBackdrops = skyBackdrops(from.sky);
  const newBackdrops = skyBackdrops(to.sky);
  const inks = [
    to.colors.text,
    to.colors.secondary,
    to.colors.tertiary,
    to.colors.accent,
    to.colors.danger,
    to.colors.read,
  ];
  for (let step = 0; step <= 8; step++) {
    for (let position = 0; position < oldBackdrops.length; position++) {
      const backdrop = mixColor(oldBackdrops[position]!, newBackdrops[position]!, step / 8);
      if (inks.some((ink) => contrastRatio(ink, backdrop) < 4.5)) return false;
    }
  }
  return true;
}

/** A foreground clock jump / timezone change should update immediately, rather
 * than animate through hours that the local clock has already skipped. */
export function sampleSkyClock(
  date: Date,
  previous?: {
    date: Date;
    automatic: ReturnType<typeof resolveSkyTheme>;
    timezoneOffset: number;
  },
) {
  const automatic = resolveSkyTheme('auto', date);
  const timezoneOffset = date.getTimezoneOffset();
  const elapsed = previous ? date.getTime() - previous.date.getTime() : 0;
  const timed =
    !!previous &&
    elapsed > 0 &&
    elapsed <= 90_000 &&
    timezoneOffset === previous.timezoneOffset &&
    canTransitionSky(previous.automatic, automatic);
  return { date, automatic, timezoneOffset, timed };
}
