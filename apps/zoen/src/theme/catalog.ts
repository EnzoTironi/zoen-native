// SPDX-License-Identifier: GPL-2.0-or-later
// Telegram-iOS f1dd7a2dbd02cbbf513e75d5695d8d36d1cf5838:
// PresentationThemeSettings and Default{Day,Dark,DarkTinted}PresentationTheme.
export const THEME_PROFILES = [
  'day',
  'night',
  'telegram-classic',
  'telegram-day',
  'telegram-night',
  'telegram-tinted',
] as const;
export type ThemeProfile = (typeof THEME_PROFILES)[number];
export function themeRecord<T>(value: T): Record<ThemeProfile, T> {
  return Object.fromEntries(THEME_PROFILES.map((profile) => [profile, value])) as Record<
    ThemeProfile,
    T
  >;
}
export function isThemeProfile(value: unknown): value is ThemeProfile {
  return THEME_PROFILES.some((profile) => profile === value);
}
export function isDarkTheme(profile: ThemeProfile): boolean {
  return profile === 'night' || profile === 'telegram-night' || profile === 'telegram-tinted';
}
/** UIKit's withMultiplied(hue:saturation:brightness:) in the upstream theme. */
function rgb(color: string): number[] {
  return [1, 3, 5].map((offset) => parseInt(color.slice(offset, offset + 2), 16) / 255);
}
function hex(channels: number[]): string {
  return `#${channels
    .map((value) =>
      Math.round(value * 255)
        .toString(16)
        .padStart(2, '0'),
    )
    .join('')}`.toUpperCase();
}
function multiplied(rgb: number[], hue: number, saturation: number, brightness: number): number[] {
  const max = Math.max(...rgb),
    min = Math.min(...rgb),
    delta = max - min;
  let h =
    delta === 0
      ? 0
      : max === rgb[0]
        ? (rgb[1]! - rgb[2]!) / delta
        : max === rgb[1]
          ? 2 + (rgb[2]! - rgb[0]!) / delta
          : 4 + (rgb[0]! - rgb[1]!) / delta;
  h = (((h / 6 + 1) % 1) * hue) % 1;
  const s = Math.min(1, (max === 0 ? 0 : delta / max) * saturation);
  const b = Math.min(1, max * brightness);
  return [5, 3, 1].map((position) => {
    const k = (position + h * 6) % 6;
    return b * (1 - s * Math.max(0, Math.min(k, 4 - k, 1)));
  });
}
export function multiplyHSB(
  color: string,
  hue: number,
  saturation: number,
  brightness: number,
): string {
  return hex(multiplied(rgb(color), hue, saturation, brightness));
}
export function tintedPalette(accent = '#2EA6FF') {
  const outgoingBottom = multiplied(rgb(accent), 1.019, 0.731, 0.59);
  return {
    block: multiplyHSB(accent, 1.024, 0.585, 0.25),
    background: multiplyHSB(accent, 1.024, 0.573, 0.18),
    input: multiplyHSB(accent, 1.029, 0.609, 0.19),
    separator: multiplyHSB(accent, 1.033, 0.426, 0.34),
    secondary: multiplyHSB(accent, 0.956, 0.17, 1),
    outgoing: [hex(multiplied(outgoingBottom, 0.966, 0.61, 0.98)), hex(outgoingBottom)],
  };
}
