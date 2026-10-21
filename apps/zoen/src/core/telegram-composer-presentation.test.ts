import assert from 'node:assert/strict';
import { test } from 'node:test';
import {
  composerFieldInsets,
  composerPreferredGlass,
  composerSendGeometry,
  composerSourceMinimumHeight,
  telegramComposer,
} from './telegram-composer-presentation.ts';

test('all seven pinned font steps drive field and send from the same single-line minimum', () => {
  for (const [font, minimum, radius, fillHeight] of [
    [14, 40, 20, 34],
    [15, 40, 20, 34],
    [16, 40, 20, 34],
    [17, 40, 20, 34],
    [19, 42, 21, 36],
    [23, 47, 23, 41],
    [26, 51, 25, 45],
  ] as const) {
    assert.equal(composerSourceMinimumHeight(font), minimum);
    const g = composerSendGeometry(minimum);
    assert.equal(g.fieldRadius, radius);
    assert.deepEqual([g.slotWidth, g.slotHeight], [46, minimum]);
    assert.deepEqual([g.fillX, g.fillY, g.fillWidth, g.fillHeight], [3, 3, 40, fillHeight]);
    assert.equal(g.fillRadius, Math.min(40, fillHeight) / 2);
  }
});
test('custom/scaled fonts are never mislabeled as a source step and invalid minimum falls back safely', () => {
  for (const size of [13, 18, 20, 21, 22, 24, 25, 17.5, NaN, Infinity])
    assert.equal(composerSourceMinimumHeight(size), null);
  assert.equal(composerSendGeometry(47).fieldRadius, 23);
  assert.equal(composerSendGeometry(NaN).fieldRadius, 20);
});
test('regular text preserves both source inset layers rather than adding padding twice', () => {
  const empty = composerFieldInsets({ fontSize: 17, hasSend: false });
  assert.deepEqual(empty.internal, { top: 5, bottom: 4, left: 12, right: 11 });
  assert.deepEqual(empty.textContainer, { top: 4.5, bottom: 5.5, left: 0, right: 0 });
  assert.deepEqual(empty.total, { top: 9.5, bottom: 9.5, left: 12, right: 11 });
  const send = composerFieldInsets({ fontSize: 17, hasSend: true });
  assert.equal(send.textContainer.right, 36);
  assert.equal(send.total.right, 47);
});
test('input-mode32pt slot differs from source measurement34pt and resulting right clearance', () => {
  const measured =
    telegramComposer.accessoryLeading +
    telegramComposer.inputModeControl -
    telegramComposer.accessoryMeasurementAdjustment;
  assert.equal(measured, 34);
  const empty = composerFieldInsets({
    fontSize: 17,
    hasSend: false,
    accessoryButtonsWidth: measured,
  });
  const send = composerFieldInsets({
    fontSize: 17,
    hasSend: true,
    accessoryButtonsWidth: measured,
  });
  assert.equal(empty.textContainer.right, 10);
  assert.equal(empty.total.right, 21);
  assert.equal(send.textContainer.right, 56);
  assert.equal(send.total.right, 67);
});
test('small-font optical insets and invalid accessory measurements stay bounded', () => {
  assert.deepEqual(composerFieldInsets({ fontSize: 14, hasSend: false }).total, {
    top: 11.5,
    bottom: 10.5,
    left: 12,
    right: 11,
  });
  for (const width of [-10, 0, NaN, Infinity])
    assert.equal(
      composerFieldInsets({ fontSize: 17, hasSend: true, accessoryButtonsWidth: width }).total
        .right,
      47,
    );
});
test('default402pt no-accessory source projection puts solid send inside its46pt slot', () => {
  const fieldX = telegramComposer.edge + telegramComposer.externalControl + telegramComposer.gap;
  const fieldRight = 402 - telegramComposer.edge;
  const send = composerSendGeometry(40);
  assert.equal(fieldX, 54);
  assert.equal(fieldRight - fieldX, 340);
  const slotX = fieldRight - send.slotWidth;
  assert.deepEqual([slotX + send.fillX, fieldRight - send.fillX], [351, 391]);
  const textEnd = fieldRight - composerFieldInsets({ fontSize: 17, hasSend: true }).total.right;
  assert.equal(textEnd, 347);
  assert.equal(slotX + send.fillX - textEnd, 4);
});
test('material selection follows actual dark+saturation policy with explicit override', () => {
  assert.equal(composerPreferredGlass({ dark: false, saturatedWallpaper: true }), 'panel');
  assert.equal(composerPreferredGlass({ dark: true, saturatedWallpaper: false }), 'panel');
  assert.equal(composerPreferredGlass({ dark: true, saturatedWallpaper: true }), 'clear');
  assert.equal(composerPreferredGlass({ dark: false, forceClear: true }), 'clear');
});
