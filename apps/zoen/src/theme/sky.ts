/** Local civil time only: no location, solar service, network, or system appearance. */
export const SKY_PHASES = ['deepNight', 'dawn', 'morning', 'day', 'sunset', 'night'] as const;
export type SkyPhase = (typeof SKY_PHASES)[number];
export type ThemePreference = 'auto' | SkyPhase;
export type SkyTextureWeights = { day: number; warm: number; night: number };
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
    phase: 'deepNight',
    minute: 0,
    top: '#080F19',
    middle: '#0D1723',
    bottom: '#14212C',
    stars: 0.35,
    haze: 0.03,
  },
  {
    phase: 'dawn',
    minute: 300,
    top: '#F2F5F8',
    middle: '#F7F4F1',
    bottom: '#FBF3E9',
    stars: 0,
    haze: 0.12,
  },
  {
    phase: 'morning',
    minute: 480,
    top: '#EEF5FA',
    middle: '#F5F8FA',
    bottom: '#FBFAF6',
    stars: 0,
    haze: 0.08,
  },
  {
    phase: 'day',
    minute: 780,
    top: '#ECF4FA',
    middle: '#F4F8FB',
    bottom: '#FBFCFC',
    stars: 0,
    haze: 0.05,
  },
  {
    phase: 'sunset',
    minute: 1080,
    top: '#F4F5F7',
    middle: '#FAF5EF',
    bottom: '#FCF2E8',
    stars: 0,
    haze: 0.1,
  },
  {
    phase: 'night',
    minute: 1260,
    top: '#0D1421',
    middle: '#121D2B',
    bottom: '#1B2934',
    stars: 0.2,
    haze: 0.05,
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
function constrainSky(sky: SkyColors): { sky: SkyColors; text: string } {
  const text = skyInk(sky);
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
): { scheme: 'light' | 'dark'; colors: SemanticColors } {
  const scheme = text === '#FFFFFF' ? 'dark' : 'light';
  const dark = scheme === 'dark';
  // Content plates remain coherent. Mixing light and dark surface colors with
  // the clock produces muddy gray cards during otherwise light afternoons.
  const background = dark ? '#0D1421' : '#F8FAFB';
  const surface = dark ? '#121D2BF5' : '#FFFFFFF5';
  const raised = dark ? '#17212DF5' : '#FFFFFFF5';
  const muted = dark ? '#1E2A36' : '#EDF1F4';
  const theirs = dark ? '#22303D' : '#F0F3F5';
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
  const accent = foreground('#9BCD6A', '#2E6B31');
  const mine = dark ? '#F3F3EE' : '#181D24';
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
      onMine: inkFor(mine),
      danger: foreground('#FF9B9D', '#AE2843'),
      read: foreground('#8CCEE9', '#225C88'),
    },
  };
}
const textureForPhase: Record<SkyPhase, keyof SkyTextureWeights> = {
  deepNight: 'night',
  dawn: 'warm',
  morning: 'day',
  day: 'day',
  sunset: 'warm',
  night: 'night',
};
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
  if (value === 'light') return 'day';
  if (value === 'dark') return 'night';
  return value === 'auto' || SKY_PHASES.some((phase) => phase === value)
    ? (value as ThemePreference)
    : 'auto';
}
export function resolveSkyTheme(preference: ThemePreference, date: Date) {
  const minute =
    date.getHours() * 60 +
    date.getMinutes() +
    date.getSeconds() / 60 +
    date.getMilliseconds() / 60_000;
  if (!Number.isFinite(minute)) throw new RangeError('A valid local clock is required.');
  const index =
    preference === 'auto'
      ? anchors.reduce((found, anchor, position) => (minute >= anchor.minute ? position : found), 0)
      : anchors.findIndex((anchor) => anchor.phase === preference);
  const from = anchors[Math.max(0, index)]!;
  const to = anchors[(index + 1) % anchors.length]!;
  const end = to.minute || 1440;
  // Hold the quiet night for the night. A five-hour midnight-to-dawn blend
  // otherwise turns the app gray at 02:30. Only actual day/night crossings use
  // a short, smooth twilight; subtle same-scheme sky changes span their phase.
  const twilight =
    (from.phase === 'deepNight' && to.phase === 'dawn') ||
    (from.phase === 'sunset' && to.phase === 'night');
  const start = twilight ? end - 30 : from.minute;
  const linear = preference === 'auto' ? clamp((minute - start) / (end - start)) : 0;
  const progress = linear * linear * (3 - 2 * linear);
  const scalar = (a: number, b: number) => a + (b - a) * progress;
  const rawSky: SkyColors = {
    top: mixColor(from.top, to.top, progress),
    middle: mixColor(from.middle, to.middle, progress),
    bottom: mixColor(from.bottom, to.bottom, progress),
    stars: scalar(from.stars, to.stars),
    haze: scalar(from.haze, to.haze),
    transitionMs: 0,
    textureWeights: Object.fromEntries(
      (['day', 'warm', 'night'] as const).map((texture) => [
        texture,
        scalar(
          textureForPhase[from.phase] === texture ? 1 : 0,
          textureForPhase[to.phase] === texture ? 1 : 0,
        ),
      ]),
    ) as SkyTextureWeights,
    textureOpacity: 0,
  };
  const { sky, text } = constrainSky(rawSky);
  // Preserve visible photography, then choose ink against that full envelope.
  // Only the primary label's immutable scheme limits opacity during twilight;
  // secondary and colored labels adapt toward that same readable primary ink.
  const cap = 0.28 - sky.textureWeights.night * 0.08;
  const photoSky = { ...sky, textureOpacity: textureOpacityFor(sky, [text], cap, 4.7) };
  const semantic = semanticColors(photoSky, text);
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
    sky: {
      ...photoSky,
      textureOpacity: textureOpacityFor(photoSky, inks, photoSky.textureOpacity),
    },
    ...semantic,
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
