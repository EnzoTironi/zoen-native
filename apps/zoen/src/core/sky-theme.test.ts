import test from 'node:test';
import assert from 'node:assert/strict';
import {
  canTransitionSky,
  contrastRatio,
  mixColor,
  normalizeAppearance,
  resolveSkyTheme,
  SKY_PHASES,
  skyBackdrops,
  sampleSkyClock,
} from '../theme/sky.ts';
const at = (minutes: number) => new Date(2026, 9, 17, 0, minutes);
test('each fixed sky is distinct and independent of clock and device appearance', () => {
  const skies = SKY_PHASES.map((phase) => {
    const morning = resolveSkyTheme(phase, at(480));
    assert.deepEqual(morning, resolveSkyTheme(phase, at(1380)));
    assert.equal(morning.phase, phase);
    assert.equal(morning.scheme, phase === 'night' ? 'dark' : 'light');
    return morning.sky.top;
  });
  assert.equal(new Set(skies).size, 2);
});

test('automatic sky selects only day and night at local06:00 and20:00, including exact boundaries', () => {
  for (const [minute, phase] of [
    [0, 'night'],
    [359, 'night'],
    [360, 'day'],
    [1199, 'day'],
    [1200, 'night'],
    [1439, 'night'],
  ] as const)
    assert.equal(resolveSkyTheme('auto', at(minute)).phase, phase);
  assert.equal(resolveSkyTheme('auto', new Date(at(360).getTime() - 1)).phase, 'night');
  assert.equal(resolveSkyTheme('auto', new Date(at(1200).getTime() - 1)).phase, 'day');
  const local = at(480);
  local.getUTCHours = () => {
    throw new Error('UTC is not the local clock');
  };
  assert.equal(resolveSkyTheme('auto', local).phase, 'day');
  assert.deepEqual(resolveSkyTheme('auto', at(0)), resolveSkyTheme('night', at(780)));
  assert.throws(() => resolveSkyTheme('auto', new Date(NaN)), RangeError);
});

test('appearance migration retains the old selection without resetting account data', () => {
  assert.equal(normalizeAppearance('system'), 'auto');
  assert.equal(normalizeAppearance('light'), 'day');
  assert.equal(normalizeAppearance('dark'), 'night');
  for (const phase of SKY_PHASES) assert.equal(normalizeAppearance(phase), phase);
  for (const value of ['dawn', 'morning', 'sunset'])
    assert.equal(normalizeAppearance(value), 'day');
  assert.equal(normalizeAppearance('deepNight'), 'night');
  for (const value of [null, '', 'obsolete', 'AUTO'])
    assert.equal(normalizeAppearance(value), 'auto');
});

test('every local minute keeps exposed sky, haze, translucent cards and paired controls readable', () => {
  for (let minute = 0; minute < 1440; minute++) {
    const { sky, colors } = resolveSkyTheme('auto', at(minute));
    const weights = Object.values(sky.textureWeights);
    assert.ok(weights.every((value) => value >= 0 && value <= 1));
    assert.ok(Math.abs(weights.reduce((sum, weight) => sum + weight, 0) - 1) < 1e-12);
    assert.ok(
      sky.textureOpacity >= 0 && sky.textureOpacity <= 0.28 - 0.08 * sky.textureWeights.night,
    );
    assert.ok(sky.stars >= 0 && sky.stars <= 1);
    assert.ok(sky.haze >= 0 && sky.haze <= 1);
    const surfaces = [
      colors.background,
      colors.muted,
      colors.theirs,
      ...[colors.surface, colors.raised].flatMap((color) =>
        ['#000000', '#FFFFFF'].map((background) => mixColor(background, color, 245 / 255)),
      ),
      ...skyBackdrops(sky),
    ];
    for (const ink of ['text', 'secondary', 'tertiary', 'accent', 'danger', 'read'] as const)
      for (const background of surfaces)
        assert.ok(
          contrastRatio(colors[ink], background) >= 4.5,
          `${minute}/${ink}: ${colors[ink]} on ${background}`,
        );
    // Default bubble colors follow the pinned source; arbitrary custom colors
    // have a separate contrast contract in theme-customization.test.ts.
    assert.equal(colors.onMine, sky.textureWeights.night ? '#FFFFFF' : '#000000');
    assert.ok(contrastRatio(colors.onAccent, colors.accent) >= 4.5, `action ${minute}`);
  }
});

test('a scheme change never fades new ink across the old sky, and an unsafe photo cannot borrow safe opacity', () => {
  const day = resolveSkyTheme('day', at(480));
  const night = resolveSkyTheme('night', at(1260));
  assert.equal(canTransitionSky(night, day), false);
  assert.equal(canTransitionSky(day, night), false);
  assert.equal(canTransitionSky(day, day), true);
  assert.equal(canTransitionSky(night, night), true);
  assert.equal(canTransitionSky({ ...day, sky: { ...day.sky, textureOpacity: 0.9 } }, day), false);
});

test('clock sampling snaps on foreground, clock jumps and timezone changes but fades normal ticks', () => {
  const first = sampleSkyClock(at(600));
  assert.equal(first.timed, false);
  const tick = sampleSkyClock(new Date(first.date.getTime() + 30_000), first);
  assert.equal(tick.timed, true);
  assert.equal(sampleSkyClock(at(900), first).timed, false);
  assert.equal(sampleSkyClock(at(599), first).timed, false);
  const changedTimezone = new Date(first.date.getTime() + 30_000);
  changedTimezone.getTimezoneOffset = () => first.timezoneOffset + 60;
  assert.equal(sampleSkyClock(changedTimezone, first).timed, false);
  assert.equal(
    sampleSkyClock(tick.date).timed,
    false,
    'foreground refresh has no prior animation authority',
  );
});

test('daytime skies stay nearly white and semantic cards never interpolate through gray', () => {
  for (const phase of ['day'] as const) {
    const theme = resolveSkyTheme(phase, at(600));
    for (const color of [theme.sky.top, theme.sky.middle, theme.sky.bottom]) {
      const channels = [1, 3, 5].map((offset) => parseInt(color.slice(offset, offset + 2), 16));
      assert.ok(Math.min(...channels) >= 230, `${phase} keeps its sky subtle`);
      assert.ok(
        Math.max(...channels) - Math.min(...channels) <= 24,
        `${phase} avoids saturated tint`,
      );
    }
  }
  const light = resolveSkyTheme('day', at(780)).colors;
  const dark = resolveSkyTheme('night', at(1260)).colors;
  for (let minute = 0; minute < 1440; minute++) {
    const theme = resolveSkyTheme('auto', at(minute));
    const expected = theme.scheme === 'light' ? light : dark;
    for (const color of ['background', 'surface', 'raised', 'muted', 'theirs', 'mine'] as const)
      assert.equal(
        theme.colors[color],
        expected[color],
        `${minute}/${color}: choose a coherent scheme, never mix plates`,
      );
  }
  assert.equal(resolveSkyTheme('auto', at(150)).scheme, 'dark', '02:30 remains a quiet night');
  assert.equal(
    resolveSkyTheme('auto', at(1047)).colors.surface,
    '#FFFFFFF5',
    '16:47 retains white cards',
  );
});

test('photo families match the two shipped skies and unknown pixels preserve sky ink contrast', () => {
  const families = { day: 'day', night: 'night' } as const;
  const blend = (background: string, foreground: string, alpha: number) =>
    `#${[1, 3, 5]
      .map((offset) => {
        const a = parseInt(background.slice(offset, offset + 2), 16);
        const b = parseInt(foreground.slice(offset, offset + 2), 16);
        return Math.round(a * (1 - alpha) + b * alpha)
          .toString(16)
          .padStart(2, '0');
      })
      .join('')}`;
  for (const phase of SKY_PHASES) {
    const { sky, colors } = resolveSkyTheme(phase, at(600));
    assert.deepEqual(sky.textureWeights, {
      day: families[phase] === 'day' ? 1 : 0,
      night: families[phase] === 'night' ? 1 : 0,
    });
    assert.equal(
      sky.textureOpacity,
      families[phase] === 'night' ? 0.2 : 0.28,
      `${phase} keeps the photograph visibly present at the intended intensity`,
    );
    for (const stop of [sky.top, sky.middle, sky.bottom])
      for (const pixel of ['#000000', '#FFFFFF', '#376FA1', '#EEBB7D', '#10C46E'])
        for (const haze of [0, sky.haze * 0.04]) {
          const painted = blend(blend(stop, pixel, sky.textureOpacity), '#FFFFFF', haze);
          for (const ink of [
            colors.text,
            colors.secondary,
            colors.tertiary,
            colors.accent,
            colors.danger,
            colors.read,
          ])
            assert.ok(
              contrastRatio(ink, painted) >= 4.5,
              `${phase}: ${ink} over arbitrary photo pixel ${pixel}`,
            );
        }
  }
  const day = resolveSkyTheme('day', at(780));
  assert.equal(
    canTransitionSky({ ...day, sky: { ...day.sky, textureOpacity: 0.9 } }, day),
    false,
    'an unsafe outgoing photo cannot be hidden behind a safe incoming opacity',
  );
});
