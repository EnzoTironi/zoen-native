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
const channelDifference = (a: string, b: string) =>
  Math.max(
    ...[1, 3, 5].map((offset) =>
      Math.abs(
        parseInt(a.slice(offset, offset + 2), 16) - parseInt(b.slice(offset, offset + 2), 16),
      ),
    ),
  );

test('each fixed sky is distinct and independent of clock and device appearance', () => {
  const skies = SKY_PHASES.map((phase) => {
    const morning = resolveSkyTheme(phase, at(480));
    assert.deepEqual(morning, resolveSkyTheme(phase, at(1380)));
    assert.equal(morning.phase, phase);
    assert.equal(morning.scheme, phase === 'deepNight' || phase === 'night' ? 'dark' : 'light');
    return morning.sky.top;
  });
  assert.equal(new Set(skies).size, 6);
});

test('automatic sky follows civil local hours, including midnight and fractional minutes', () => {
  for (const [minute, phase] of [
    [0, 'deepNight'],
    [300, 'dawn'],
    [480, 'morning'],
    [780, 'day'],
    [1080, 'sunset'],
    [1260, 'night'],
  ] as const) {
    assert.equal(resolveSkyTheme('auto', at(minute)).phase, phase);
    const before = resolveSkyTheme('auto', new Date(at(minute).getTime() - 1));
    const after = resolveSkyTheme('auto', at(minute));
    for (const stop of ['top', 'middle', 'bottom'] as const)
      assert.ok(
        channelDifference(before.sky[stop], after.sky[stop]) <= 1,
        `${phase}/${stop} must join continuously`,
      );
  }
  const local = at(480);
  local.getUTCHours = () => {
    throw new Error('UTC is not the local clock');
  };
  assert.equal(resolveSkyTheme('auto', local).phase, 'morning');
  assert.equal(resolveSkyTheme('auto', at(779)).phase, 'morning');
  assert.equal(resolveSkyTheme('auto', at(780)).phase, 'day');
  assert.throws(() => resolveSkyTheme('auto', new Date(NaN)), RangeError);
});

test('appearance migration retains the old selection without resetting account data', () => {
  assert.equal(normalizeAppearance('system'), 'auto');
  assert.equal(normalizeAppearance('light'), 'day');
  assert.equal(normalizeAppearance('dark'), 'night');
  for (const phase of SKY_PHASES) assert.equal(normalizeAppearance(phase), phase);
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
    assert.ok(contrastRatio(colors.onMine, colors.mine) >= 4.5, `own message ${minute}`);
    assert.ok(contrastRatio(colors.onAccent, colors.accent) >= 4.5, `action ${minute}`);
  }
});

test('independently animated sky cannot show new ink against an unreadable outgoing layer', () => {
  const sunset = resolveSkyTheme('sunset', at(1080));
  const morning = resolveSkyTheme('morning', at(480));
  assert.equal(
    canTransitionSky(sunset, morning),
    true,
    'subtle fixed light skies share enough contrast for a safe fade',
  );
  const twilight = resolveSkyTheme('auto', at(285));
  assert.equal(twilight.scheme, morning.scheme);
  assert.equal(
    canTransitionSky(twilight, morning),
    false,
    'new secondary ink must still read on an outgoing twilight',
  );
  assert.equal(canTransitionSky(resolveSkyTheme('night', at(1260)), morning), false);
  assert.equal(canTransitionSky(morning, morning), true);
  let smooth = 0;
  for (let minute = 0; minute < 1440; minute += 5) {
    const from = resolveSkyTheme('auto', at(minute));
    const to = resolveSkyTheme('auto', new Date(at(minute).getTime() + 30_000));
    if (!canTransitionSky(from, to)) continue;
    smooth++;
    for (const background of skyBackdrops(from.sky))
      for (const ink of [to.colors.text, to.colors.secondary, to.colors.tertiary])
        assert.ok(contrastRatio(ink, background) >= 4.5);
  }
  assert.ok(smooth > 250, 'ordinary clock progression should remain smooth, not disable all fades');
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
  for (const phase of ['dawn', 'morning', 'day', 'sunset'] as const) {
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

test('photo families match the six moments and unknown pixels preserve sky ink contrast', () => {
  const families = {
    deepNight: 'night',
    dawn: 'warm',
    morning: 'day',
    day: 'day',
    sunset: 'warm',
    night: 'night',
  } as const;
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
      warm: families[phase] === 'warm' ? 1 : 0,
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
