/*
 * Zoen living sky for the docs site: a decorative sky that follows the visitor's local time.
 *
 * The palette, time anchors, star curve and smoothstep easing are copied verbatim from the app's
 * SkyClock (PR #127, branch ui/living-sky, commit 3d86d3b):
 *   apple/Shared/DesignSystem/LivingSky.swift · android/…/theme/LivingSky.kt · packages/zoen-ui/web/living-sky.ts
 * Keep these tokens identical to that model. Only the wash strength differs (the app tints its
 * canvas 8%; the site washes the top of the page more strongly, see custom.css).
 *
 * Local civil time only: no location, network, sunrise API or permissions. Runs as a small classic
 * script in <head> so the first paint already has the right sky. No timers while the tab is hidden.
 */
(function () {
  'use strict';

  /** Hour anchors (local civil time) and their sky colours, as linear sRGB 0…1 triples. */
  var HOURS = [0, 6, 9, 12, 17, 20, 24];
  var COLORS = [
    [0.20, 0.23, 0.46], // 00:00 night
    [0.84, 0.48, 0.53], // 06:00 dawn
    [0.38, 0.67, 0.87], // 09:00 morning
    [0.27, 0.62, 0.90], // 12:00 midday
    [0.94, 0.61, 0.30], // 17:00 golden hour
    [0.51, 0.34, 0.63], // 20:00 dusk
    [0.20, 0.23, 0.46], // 24:00 night (wraps to 00:00)
  ];
  /** Star visibility at each anchor (1 = full night). */
  var STARS = [1, 0, 0, 0, 0, 0.4, 1];

  function sample(hour) {
    var h = isFinite(hour) ? ((hour % 24) + 24) % 24 : 12;
    var i = 0;
    while (i < HOURS.length - 2 && h >= HOURS[i + 1]) i++;
    var t = (h - HOURS[i]) / (HOURS[i + 1] - HOURS[i]);
    var s = t * t * (3 - 2 * t);
    var rgb = [0, 1, 2].map(function (c) { return COLORS[i][c] + (COLORS[i + 1][c] - COLORS[i][c]) * s; });
    return { rgb: rgb, stars: STARS[i] + (STARS[i + 1] - STARS[i]) * s };
  }

  /** Coarse name for the moment, exposed as html[data-sky-phase] for styling and tests. */
  function phase(hour) {
    var h = ((hour % 24) + 24) % 24;
    if (h >= 5 && h < 8) return 'dawn';
    if (h >= 8 && h < 16) return 'day';
    if (h >= 16 && h < 20) return 'sunset';
    return 'night';
  }

  /*
   * Contrast guard. The wash at the top of the page is as strong as the theme allows (up to MAX)
   * but never pushes the background past a luminance bound that keeps the theme's text at
   * WCAG 2.2 AA with margin: light pages stay at or above L 0.50 (muted text #39453e needs 0.42
   * for 4.5:1), dark pages at or below L 0.05 (muted text #c1c3c8 needs 0.08).
   */
  var THEMES = {
    light: { base: [255, 255, 255], max: 0.46, ok: function (l) { return l >= 0.50; } },
    dark: { base: [12, 18, 16], max: 0.34, ok: function (l) { return l <= 0.05; } },
  };
  function luminance(rgb) {
    var c = rgb.map(function (v) { v /= 255; return v <= 0.04045 ? v / 12.92 : Math.pow((v + 0.055) / 1.055, 2.4); });
    return 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
  }
  function mix(sky255, base, w) { return base.map(function (b, i) { return sky255[i] * w + b * (1 - w); }); }
  function wash(rgb, theme) {
    var t = THEMES[theme] || THEMES.light;
    var sky255 = rgb.map(function (v) { return v * 255; });
    if (t.ok(luminance(mix(sky255, t.base, t.max)))) return t.max;
    var lo = 0, hi = t.max;
    for (var k = 0; k < 20; k++) { var m = (lo + hi) / 2; if (t.ok(luminance(mix(sky255, t.base, m)))) lo = m; else hi = m; }
    return lo;
  }

  var api = { HOURS: HOURS, COLORS: COLORS, STARS: STARS, THEMES: THEMES, sample: sample, phase: phase, wash: wash, mix: mix, luminance: luminance };
  if (typeof globalThis !== 'undefined') globalThis.ZoenSky = api;
  if (typeof document === 'undefined') return; // imported by tests

  var root = document.documentElement;
  var timer;

  function currentHour() {
    // Loopback-only preview override, the same contract as the web app (?skyHour=17).
    if (['localhost', '127.0.0.1', '[::1]'].indexOf(location.hostname) !== -1) {
      var raw = new URLSearchParams(location.search).get('skyHour');
      if (raw !== null && raw !== '' && isFinite(Number(raw))) return Number(raw);
    }
    var now = new Date();
    return now.getHours() + now.getMinutes() / 60 + now.getSeconds() / 3600;
  }

  function update() {
    clearTimeout(timer);
    if (document.hidden) return;
    var hour = currentHour();
    var sky = sample(hour);
    var rgb = sky.rgb.map(function (v) { return Math.round(v * 255); }).join(' ');
    root.style.setProperty('--zoen-sky', 'rgb(' + rgb + ')');
    root.style.setProperty('--zoen-sky-stars', sky.stars.toFixed(3));
    var w = wash(sky.rgb, root.getAttribute('data-theme') === 'dark' ? 'dark' : 'light');
    root.style.setProperty('--zoen-sky-wash-top', (w * 100).toFixed(1) + '%');
    root.style.setProperty('--zoen-sky-wash-mid', (w * 45).toFixed(1) + '%');
    root.setAttribute('data-sky-phase', phase(hour));
    timer = setTimeout(update, 60000);
  }

  // Deterministic decoration, same layout rules as the app's Canvas: five ink clouds, 36 cross stars.
  function paint(layer) {
    var w = Math.max(window.innerWidth, 320), h = Math.max(window.innerHeight, 320);
    var ns = 'http://www.w3.org/2000/svg';
    var svg = document.createElementNS(ns, 'svg');
    svg.setAttribute('width', '100%');
    svg.setAttribute('height', '100%');
    svg.setAttribute('viewBox', '0 0 ' + w + ' ' + h);
    svg.setAttribute('focusable', 'false');
    var clouds = '', stars = '';
    for (var i = 0; i < 5; i++) {
      var x = (w * i) / 4, y = h * (0.10 + (i % 3) * 0.12);
      clouds += 'M' + (x - 65) + ' ' + y + 'C' + (x - 30) + ' ' + (y - 24) + ' ' + (x + 20) + ' ' + (y + 16) + ' ' + (x + 70) + ' ' + (y + 3);
    }
    for (var j = 0; j < 36; j++) {
      var sx = (w * ((j * 73 + 19) % 997)) / 997, sy = (h * ((j * 137 + 31) % 631)) / 1000;
      stars += 'M' + (sx - 2) + ' ' + sy + 'L' + (sx + 2) + ' ' + (sy + 0.5) + 'M' + sx + ' ' + (sy - 2) + 'L' + (sx + 0.5) + ' ' + (sy + 2);
    }
    svg.innerHTML = '<path class="zoen-sky-clouds" d="' + clouds + '"/><path class="zoen-sky-stars" d="' + stars + '"/>';
    layer.replaceChildren(svg);
  }

  function mount() {
    if (document.querySelector('.zoen-sky')) return;
    var layer = document.createElement('div');
    layer.className = 'zoen-sky';
    layer.setAttribute('aria-hidden', 'true'); // purely decorative
    document.body.prepend(layer);
    paint(layer);
    requestAnimationFrame(function () { requestAnimationFrame(function () { root.setAttribute('data-sky-live', ''); }); });
    var resizeTimer;
    window.addEventListener('resize', function () {
      clearTimeout(resizeTimer);
      resizeTimer = setTimeout(function () { paint(layer); }, 150);
    });
  }

  update();
  document.addEventListener('visibilitychange', update);
  // Re-guard the wash when the reader switches light/dark.
  new MutationObserver(update).observe(root, { attributes: true, attributeFilter: ['data-theme'] });
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', mount);
  else mount();
})();
