import test from 'node:test';
import assert from 'node:assert/strict';
import { PHOTO_CARD_SCRIM_OPACITY, PHOTO_CARD_SECONDARY, PHOTO_CARD_TEXT } from './photo-card.ts';

const luminance = (rgb: readonly number[]) =>
  rgb.reduce((sum, value, index) => {
    const channel = value / 255;
    return (
      sum +
      (channel <= 0.04045 ? channel / 12.92 : ((channel + 0.055) / 1.055) ** 2.4) *
        [0.2126, 0.7152, 0.0722][index]!
    );
  }, 0);
const rgb = (hex: string) => [1, 3, 5].map((offset) => parseInt(hex.slice(offset, offset + 2), 16));
const contrast = (a: readonly number[], b: readonly number[]) => {
  const values = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (values[0]! + 0.05) / (values[1]! + 0.05);
};

test('photo captions remain readable over every image color, including a white hiking sky', () => {
  // Black scrim composition is monotonic per channel; white is the brightest
  // possible backdrop. Sample intermediate colors independently as well.
  const worst = [255, 255, 255].map((channel) => channel * (1 - PHOTO_CARD_SCRIM_OPACITY));
  for (const ink of [PHOTO_CARD_TEXT, PHOTO_CARD_SECONDARY]) {
    assert.ok(contrast(rgb(ink), worst) >= 4.5, `${ink} over a white photograph`);
    for (const red of [0, 64, 128, 192, 255])
      for (const green of [0, 64, 128, 192, 255])
        for (const blue of [0, 64, 128, 192, 255]) {
          const backdrop = [red, green, blue].map(
            (channel) => channel * (1 - PHOTO_CARD_SCRIM_OPACITY),
          );
          assert.ok(contrast(rgb(ink), backdrop) >= 4.5);
        }
  }
  // The former 5% top overlay fails even for primary white text.
  assert.ok(contrast(rgb(PHOTO_CARD_TEXT), [242.25, 242.25, 242.25]) < 1.2);
});
