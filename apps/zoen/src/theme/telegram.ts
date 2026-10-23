// SPDX-License-Identifier: GPL-2.0-or-later
// Translated presentation values and customization rules from Telegram-iOS,
// revision f1dd7a2dbd02cbbf513e75d5695d8d36d1cf5838, Telegram contributors.
// DefaultDayPresentationTheme.swift, DefaultDarkPresentationTheme.swift,
// ChatTextInputPanelNode, NavigationBarImpl and CAAnimationUtils.
// The selected GPLv3 option and attribution are in THIRD_PARTY_NOTICES.md.

export const TELEGRAM_UI_REVISION = 'f1dd7a2dbd02cbbf513e75d5695d8d36d1cf5838';
import { isDarkTheme, multiplyHSB, tintedPalette, type ThemeProfile } from './catalog.ts';
export type { ThemeProfile } from './catalog.ts';
export type ThemeCustomization = Readonly<{
  accent: string | null;
  outgoing: string | null;
  backgroundTop: string | null;
  backgroundBottom: string | null;
  textSize: number;
  largeEmoji: boolean;
  reduceMotion: boolean;
}>;
export const DEFAULT_THEME_CUSTOMIZATION: ThemeCustomization = Object.freeze({
  accent: null,
  outgoing: null,
  backgroundTop: null,
  backgroundBottom: null,
  textSize: 17,
  largeEmoji: true,
  reduceMotion: false,
});
const hex = (value: unknown): value is string =>
  typeof value === 'string' && /^#[\da-f]{6}$/i.test(value);
/** Keep a custom accent/bubble legible even at the extremes of the native color picker. */
function colorLuminance(color: string): number {
  const channels = [1, 3, 5].map((offset) => {
    const value = parseInt(color.slice(offset, offset + 2), 16) / 255;
    return value <= 0.04045 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4;
  });
  return channels[0]! * 0.2126 + channels[1]! * 0.7152 + channels[2]! * 0.0722;
}
export function contrastingInk(color: string): string {
  const luminance = colorLuminance(color);
  return (luminance + 0.05) / 0.05 >= 1.05 / (luminance + 0.05) ? '#000000' : '#FFFFFF';
}
function customForegroundAccent(color: string, dark: boolean): string {
  const surfaces = dark
    ? ['#000000', '#1C1C1D', '#2C2C2E', '#343434']
    : ['#FFFFFF', '#EFEFF4', '#DBDBDB'];
  const target = dark ? 255 : 0;
  const channels = [1, 3, 5].map((offset) => parseInt(color.slice(offset, offset + 2), 16));
  for (let step = 0; step <= 100; step++) {
    const next = `#${channels
      .map((value) =>
        Math.round(value + ((target - value) * step) / 100)
          .toString(16)
          .padStart(2, '0'),
      )
      .join('')}`.toUpperCase();
    if (
      surfaces.every((surface) => {
        const a = colorLuminance(next),
          b = colorLuminance(surface);
        return (Math.max(a, b) + 0.05) / (Math.min(a, b) + 0.05) >= 4.5;
      })
    )
      return next;
  }
  return dark ? '#FFFFFF' : '#000000';
}

/** Device presentation only; unknown/corrupt saved preferences cannot become native props. */
export function normalizeThemeCustomization(value: unknown): ThemeCustomization {
  if (!value || typeof value !== 'object' || Array.isArray(value))
    return DEFAULT_THEME_CUSTOMIZATION;
  const input = value as Record<string, unknown>;
  return {
    accent: hex(input.accent) ? input.accent.toUpperCase() : null,
    outgoing: hex(input.outgoing) ? input.outgoing.toUpperCase() : null,
    backgroundTop: hex(input.backgroundTop) ? input.backgroundTop.toUpperCase() : null,
    backgroundBottom: hex(input.backgroundBottom) ? input.backgroundBottom.toUpperCase() : null,
    textSize:
      typeof input.textSize === 'number' &&
      Number.isFinite(input.textSize) &&
      input.textSize >= 13 &&
      input.textSize <= 26
        ? Math.round(input.textSize)
        : 17,
    largeEmoji: typeof input.largeEmoji === 'boolean' ? input.largeEmoji : true,
    reduceMotion: typeof input.reduceMotion === 'boolean' ? input.reduceMotion : false,
  };
}
export function decodeThemeCustomization(value: string | null): ThemeCustomization {
  if (!value || value.length > 2048) return DEFAULT_THEME_CUSTOMIZATION;
  try {
    return normalizeThemeCustomization(JSON.parse(value));
  } catch {
    return DEFAULT_THEME_CUSTOMIZATION;
  }
}

export const telegramTypography = Object.freeze({
  largeTitle: { fontSize: 34, fontWeight: '700' as const },
  title: { fontSize: 17, fontWeight: '600' as const },
  body: { fontSize: 17, fontWeight: '400' as const },
  secondary: { fontSize: 15, fontWeight: '400' as const },
  footnote: { fontSize: 13, fontWeight: '400' as const },
  caption: { fontSize: 12, fontWeight: '400' as const },
  timestamp: { fontSize: 11, fontWeight: '400' as const },
});
export const telegramMetrics = Object.freeze({
  control: 44,
  navigation: 60,
  navigationControlTop: 10,
  edge: 16,
  titleClearance: 10,
  composer: 40,
  composerGap: 6,
  composerEdge: 8,
  bubbleRadius: 16,
  bubbleMergedRadius: 8,
  bubbleTextHorizontal: 11,
  bubbleTextVertical: 6,
  avatar: 38,
  contextAction: 44,
});
export const telegramMotion = Object.freeze({
  replyReturn: { duration: 300, bezier: [0.38, 0.7, 0.125, 1] as const },
  receipt: { duration: 100, fromScale: 1.3 },
  context: {
    delay: 120,
    hold: 200,
    extraction: 420,
    mass: 5,
    stiffness: 900,
    damping: 104,
    actionFade: 50,
    reactionFade: 150,
    dismissal: 200,
    reactionDismissal: 250,
  },
});

/** Pure UIKit RGB/HSB translation used by the Classic accent customization branch. */
function classicBubble(accent: string): string {
  const rgb = [1, 3, 5].map((offset) => parseInt(accent.slice(offset, offset + 2), 16) / 255);
  const high = Math.max(...rgb),
    low = Math.min(...rgb),
    delta = high - low;
  let hue = 0;
  if (delta > 0) {
    const index = rgb.indexOf(high);
    hue =
      (index === 0
        ? (rgb[1]! - rgb[2]!) / delta
        : index === 1
          ? 2 + (rgb[2]! - rgb[0]!) / delta
          : 4 + (rgb[0]! - rgb[1]!) / delta) / 6;
    if (hue < 0) hue += 1;
  }
  const saturation = high > 0 && delta > 0 ? 0.14 : 0;
  const brightness = 0.79 + high * 0.21;
  const component = (position: number) => {
    const k = (position + hue * 6) % 6;
    return brightness * (1 - saturation * Math.max(0, Math.min(k, 4 - k, 1)));
  };
  return `#${[5, 3, 1]
    .map((position) =>
      Math.round(component(position) * 255)
        .toString(16)
        .padStart(2, '0'),
    )
    .join('')}`.toUpperCase();
}

/** Shared presentation for the two skies and all four Telegram built-in families. */
export function telegramPresentation(
  profile: ThemeProfile,
  settings = DEFAULT_THEME_CUSTOMIZATION,
  skyAccent?: string,
) {
  const dark = isDarkTheme(profile);
  const tinted = profile === 'telegram-tinted' ? tintedPalette(settings.accent ?? undefined) : null;
  const coloredDay = profile === 'telegram-day';
  // PresentationData's default auto-Night selects the blue accent (#3E88F7).
  // Keep the action fill/glyph coherent with a colored outgoing-bubble theme.
  const selectedAccent =
    settings.accent ??
    (tinted ? '#2EA6FF' : profile === 'telegram-night' ? '#FFFFFF' : dark ? '#3E88F7' : '#0088FF');
  const accent = settings.accent
    ? (skyAccent ?? customForegroundAccent(selectedAccent, dark))
    : selectedAccent;
  const text = dark ? '#FFFFFF' : '#000000';
  const secondary = tinted?.secondary ?? (dark ? '#98989E' : '#8E8E93');
  const separator = tinted?.separator ?? (dark ? '#5454588C' : '#C8C7CC');
  const destructive = dark ? '#EB5545' : '#FF3B30';
  const outgoing = settings.outgoing
    ? [settings.outgoing]
    : tinted
      ? tinted.outgoing
      : coloredDay
        ? [multiplyHSB(selectedAccent, 0.966, 0.61, 0.98), selectedAccent]
        : dark
          ? ['#61BCF9', '#0088FF']
          : [settings.accent ? classicBubble(settings.accent) : '#E1FFC7'];
  const outgoingText = settings.outgoing
    ? contrastingInk(settings.outgoing)
    : dark || coloredDay
      ? '#FFFFFF'
      : '#000000';
  const actionText =
    settings.accent || profile === 'telegram-night' ? contrastingInk(selectedAccent) : '#FFFFFF';
  const customOutgoing = !!settings.outgoing;
  const coloredOutgoing = dark || coloredDay;
  const outgoingSecondary = customOutgoing
    ? `${outgoingText}99`
    : coloredOutgoing
      ? '#FFFFFF80'
      : '#008C09CC';
  const incomingFill = tinted ? `${tinted.block}E6` : dark ? '#1D1D1DE6' : '#FFFFFF';
  return {
    profile,
    dark,
    accent,
    selectedAccent,
    text,
    secondary,
    destructive,
    separator,
    largeEmoji: settings.largeEmoji,
    typography: { ...telegramTypography, message: { fontSize: settings.textSize } },
    metrics: telegramMetrics,
    motion: telegramMotion,
    navigation: {
      background: tinted ? `${tinted.block}E6` : dark ? '#1D1D1DE6' : '#F2F2F2E6',
      text,
      secondary: dark ? '#FFFFFF80' : '#787878',
      control: accent,
      disabled: dark ? '#525252' : '#D0D0D0',
      separator,
      badge: dark ? '#FFFFFF' : '#FF3B30',
      badgeText: dark ? '#000000' : '#FFFFFF',
    },
    list: {
      background: tinted?.background ?? (dark ? '#000000' : '#EFEFF4'),
      plain: tinted?.background ?? (dark ? '#000000' : '#FFFFFF'),
      block: tinted?.block ?? (dark ? '#1C1C1D' : '#FFFFFF'),
      modal: tinted?.block ?? (dark ? '#2C2C2E' : '#FFFFFF'),
      highlighted: dark ? '#313135' : '#E5E5EA',
      text,
      secondary,
      section: dark ? '#8D8E93' : '#6D6D72',
      disclosure: dark ? '#FFFFFF47' : '#BAB9BE',
      separator,
      accent,
      destructive,
    },
    chatList: {
      background: tinted?.background ?? (dark ? '#000000' : '#FFFFFF'),
      pinned: dark ? '#1C1C1D' : '#F7F7F7',
      highlighted: dark ? '#121212' : '#E5E5EA',
      title: text,
      message: dark ? '#8D8E93' : '#8E8E93',
      date: dark ? '#8D8E93' : '#8E8E93',
      unread: selectedAccent,
      unreadText: actionText,
      mutedUnread: dark ? '#666666' : '#B6B6BB',
      draft: '#DD4B39',
      pinnedIcon: dark ? '#767677' : '#B6B6BB',
    },
    search: {
      background: dark ? '#272728' : '#0000000F',
      text,
      placeholder: dark ? '#8F8F8F' : '#8E8E93',
      icon: dark ? '#8F8F8F' : '#8E8E93',
      accent,
    },
    chat: {
      incoming: {
        fill: incomingFill,
        fillColors: [incomingFill],
        text,
        secondary: dark ? '#FFFFFF80' : '#77828C',
        accent,
        link: dark ? accent : '#004BAD',
        reaction: dark ? '#FFFFFF12' : '#0088FF1A',
      },
      outgoing: {
        fill: outgoing[0]!,
        fillColors: outgoing,
        text: outgoingText,
        secondary: outgoingSecondary,
        accent: customOutgoing ? outgoingText : coloredOutgoing ? '#FFFFFF' : '#00A700',
        link: customOutgoing ? outgoingText : coloredOutgoing ? '#FFFFFF' : '#004BAD',
        reaction: customOutgoing ? `${outgoingText}1A` : dark ? '#FFFFFF1A' : '#3FC33B1F',
      },
      service: { background: dark ? '#1F1F1F' : '#FFFFFFCC', text: dark ? '#FFFFFF' : '#6D6D72' },
      input: {
        fill: tinted ? `${tinted.input}F2` : dark ? '#242424F2' : '#FFFFFFCC',
        stroke: dark ? '#FFFFFF1A' : '#0000001A',
        text,
        placeholder: dark ? '#FFFFFF7A' : '#00000066',
        control: dark ? '#FFFFFF80' : '#00000080',
        panelControl: dark ? '#FFFFFF' : '#000000',
        action: selectedAccent,
        actionText,
        recording: dark ? '#EB5545' : '#ED2521',
      },
    },
    context: {
      background: dark ? '#252525C7' : '#F9F9F9C7',
      highlight: dark ? '#FFFFFF26' : '#3C3C4333',
      text,
      secondary: dark ? '#FFFFFF80' : '#00000080',
      separator: dark ? '#FFFFFF26' : '#3C3C4333',
      destructive,
      dim: dark ? '#00000099' : '#000A2633',
      isDark: dark,
    },
    actionSheet: {
      background: dark ? '#1C1C1DCC' : '#FFFFFFCC',
      highlighted: dark ? '#00000080' : '#E6E6E6B3',
      text,
      accent,
      destructive,
      separator,
      dim: dark ? '#00000080' : '#00000066',
    },
  };
}
export type TelegramPresentation = ReturnType<typeof telegramPresentation>;
