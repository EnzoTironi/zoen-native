import assert from 'node:assert/strict';
import test from 'node:test';
import {
  DEFAULT_THEME_CUSTOMIZATION as defaults,
  decodeThemeCustomization,
  normalizeThemeCustomization,
  telegramPresentation,
} from '../theme/telegram.ts';
import { THEME_PROFILES, isDarkTheme, multiplyHSB } from '../theme/catalog.ts';
import { contrastRatio, resolveSkyTheme, skyBackdrops } from '../theme/sky.ts';

test('saved presentation rejects malformed colors, nonfinite sizes and foreign fields before native props', () => {
  for (const value of [null, 'broken', '[]', '{', 'x'.repeat(2049)])
    assert.deepEqual(decodeThemeCustomization(value), defaults);
  assert.deepEqual(
    normalizeThemeCustomization({
      accent: 'red',
      outgoing: '#123',
      backgroundTop: 'file:///private',
      textSize: Infinity,
      largeEmoji: 'false',
      reduceMotion: 1,
    }),
    defaults,
  );
  assert.deepEqual(
    normalizeThemeCustomization({
      ...defaults,
      accent: '#aAbB01',
      textSize: 19.6,
      foreign: 'discard',
    }),
    { ...defaults, accent: '#AABB01', textSize: 20 },
  );
  for (const textSize of [NaN, -1, 12, 27])
    assert.equal(normalizeThemeCustomization({ textSize }).textSize, 17);
  const selected = {
    ...defaults,
    outgoing: '#000000',
    backgroundTop: '#09131F',
    textSize: 23,
    largeEmoji: false,
    reduceMotion: true,
  };
  assert.deepEqual(decodeThemeCustomization(JSON.stringify(selected)), selected);
});

test('existing sky families retain pinned palette, typography and motion', () => {
  const day = telegramPresentation('day');
  const night = telegramPresentation('night');
  assert.equal(day.accent, '#0088FF');
  assert.equal(day.chat.incoming.fill, '#FFFFFF');
  assert.deepEqual(day.chat.outgoing.fillColors, ['#E1FFC7']);
  assert.deepEqual(night.chat.outgoing.fillColors, ['#61BCF9', '#0088FF']);
  assert.equal(night.chat.incoming.fill, '#1D1D1DE6');
  assert.equal(night.accent, '#3E88F7');
  assert.equal(night.chat.input.actionText, '#FFFFFF');
  assert.equal(day.typography.message.fontSize, 17);
  assert.equal(day.typography.title.fontSize, 17);
  assert.deepEqual(day.motion.replyReturn.bezier, [0.38, 0.7, 0.125, 1]);
  assert.equal(day.motion.context.extraction, 420);
});

test('custom colors and text affect the actual shared presentation, including extreme picker selections', () => {
  for (const profile of THEME_PROFILES) {
    for (const color of ['#000000', '#FFFFFF', '#0088FF', '#FEDD00', '#757575', '#FEA1DE']) {
      const theme = telegramPresentation(profile, {
        ...defaults,
        accent: color,
        outgoing: color,
        textSize: 26,
        largeEmoji: false,
      });
      assert.equal(theme.selectedAccent, color);
      assert.ok(contrastRatio(theme.list.accent, theme.list.block) >= 4.5);
      assert.ok(
        contrastRatio(theme.navigation.control, isDarkTheme(profile) ? '#343434' : '#DBDBDB') >=
          4.5,
      );
      assert.equal(theme.chat.outgoing.fill, color);
      assert.deepEqual(theme.chat.outgoing.fillColors, [color]);
      assert.ok(contrastRatio(theme.chat.outgoing.text, color) >= 4.5);
      assert.ok(contrastRatio(theme.chat.input.actionText, color) >= 4.5);
      assert.ok(contrastRatio(theme.chatList.unreadText, color) >= 4.5);
      assert.equal(theme.typography.message.fontSize, 26);
      assert.equal(theme.largeEmoji, false);
    }
  }
  assert.equal(
    telegramPresentation('day', { ...defaults, accent: '#0088FF' }).chat.outgoing.fill,
    '#DBEEFF',
  );
});

test('custom sky colors remain scoped to a family and preserve ink against every possible photo pixel', () => {
  for (const profile of ['day', 'night'] as const) {
    const actual = resolveSkyTheme(profile, new Date(2026, 9, 20), {
      ...defaults,
      backgroundTop: '#FEDAF1',
      backgroundBottom: '#002244',
    });
    assert.equal(actual.phase, profile);
    assert.equal(actual.scheme, profile === 'night' ? 'dark' : 'light');
    for (const background of skyBackdrops(actual.sky))
      for (const role of ['text', 'secondary', 'tertiary', 'accent', 'danger', 'read'] as const)
        assert.ok(contrastRatio(actual.colors[role], background) >= 4.5, `${profile}/${role}`);
    assert.notDeepEqual(actual.sky, resolveSkyTheme(profile, new Date(2026, 9, 20)).sky);
  }
});

test('Telegram built-in palettes render through the same presentation without sky photos or losing explicit selection', () => {
  const date = new Date(2026, 9, 20, 12);
  for (const profile of THEME_PROFILES) {
    const actual = resolveSkyTheme(profile, date);
    assert.equal(actual.profile, profile);
    assert.equal(actual.presentation.profile, profile);
    assert.equal(actual.scheme, isDarkTheme(profile) ? 'dark' : 'light');
    if (profile.startsWith('telegram-')) {
      assert.equal(actual.sky.textureOpacity, 0);
      assert.deepEqual(actual.sky.textureWeights, { day: 0, night: 0 });
    }
    for (const background of skyBackdrops(actual.sky))
      assert.ok(contrastRatio(actual.colors.text, background) >= 4.5, profile);
  }
  const classic = telegramPresentation('telegram-classic');
  const day = telegramPresentation('telegram-day');
  const night = telegramPresentation('telegram-night');
  const tinted = telegramPresentation('telegram-tinted');
  assert.equal(classic.chat.outgoing.fill, '#E1FFC7');
  assert.deepEqual(day.chat.outgoing.fillColors, ['#61C5FA', '#0088FF']);
  assert.equal(night.accent, '#FFFFFF');
  assert.equal(night.chat.input.actionText, '#000000');
  assert.equal(tinted.selectedAccent, '#2EA6FF');
  assert.equal(tinted.list.block, '#213040');
  assert.notEqual(tinted.chat.incoming.fill, night.chat.incoming.fill);
  assert.equal(multiplyHSB('#808080', 1, 1, 0.5), '#404040');
});
