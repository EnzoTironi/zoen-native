/*
 * Zoen living sky for the docs site: real time-lapse footage of the sky that follows the visitor's
 * local time. Four looping clips (dawn, day, sunset, night; sources and licences in CREDITS.md)
 * crossfade around the phase boundaries. The footage is softly blurred and toned by custom.css,
 * and all text sits on frosted panels, so the sky never decides whether text is readable.
 *
 * Local civil time only: no location, network lookups or permissions. Runs as a small classic
 * script in <head> so the first paint already has the right poster image.
 * Reduced motion: still photos only, no video is downloaded. Hidden tab: videos pause.
 */
(function () {
  'use strict';

  var PHASES = ['night', 'dawn', 'day', 'sunset'];
  /** Where each phase starts (local hour). A phase hands over to the next across BLEND hours. */
  var STARTS = { night: 20, dawn: 5, day: 8, sunset: 16 };
  var BLEND = 1; // hours of crossfade, centred on each boundary

  function norm(h) { return isFinite(h) ? ((h % 24) + 24) % 24 : 12; }
  function smooth(t) { t = Math.min(1, Math.max(0, t)); return t * t * (3 - 2 * t); }

  /** Coarse name for the moment, exposed as html[data-sky-phase]. */
  function phase(hour) {
    var h = norm(hour);
    if (h >= 5 && h < 8) return 'dawn';
    if (h >= 8 && h < 16) return 'day';
    if (h >= 16 && h < 20) return 'sunset';
    return 'night';
  }

  /** Opacity of each clip at this hour; always sums to 1, at most two clips are visible. */
  function weights(hour) {
    var h = norm(hour), w = { night: 0, dawn: 0, day: 0, sunset: 0 };
    var order = ['dawn', 'day', 'sunset', 'night'];
    for (var i = 0; i < order.length; i++) {
      var next = order[i], prev = order[(i + 3) % 4];
      var d = h - STARTS[next];
      if (d > 12) d -= 24; if (d < -12) d += 24;
      if (Math.abs(d) < BLEND / 2) { var t = smooth(d / BLEND + 0.5); w[next] = t; w[prev] = 1 - t; w.from = prev; w.to = next; return w; }
    }
    w[phase(h)] = 1;
    w.from = w.to = phase(h);
    return w;
  }

  var api = { PHASES: PHASES, STARTS: STARTS, BLEND: BLEND, phase: phase, weights: weights };
  if (typeof globalThis !== 'undefined') globalThis.ZoenSky = api;
  if (typeof document === 'undefined') return; // imported by tests

  var root = document.documentElement;
  var still = window.matchMedia('(prefers-reduced-motion: reduce)');
  var layers = {};
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

  function layer(name) {
    if (layers[name]) return layers[name];
    var el = document.createElement('div');
    el.className = 'zoen-sky-layer';
    el.style.backgroundImage = 'url(/sky/' + name + '.jpg)';
    layers[name] = el;
    document.querySelector('.zoen-sky').append(el);
    return el;
  }

  function video(name, el) {
    var v = el.querySelector('video');
    if (still.matches) { if (v) v.remove(); return null; }
    if (!v) {
      v = document.createElement('video');
      v.muted = true; v.loop = true; v.playsInline = true; v.autoplay = true;
      v.setAttribute('muted', ''); v.setAttribute('playsinline', ''); v.setAttribute('preload', 'auto');
      v.poster = '/sky/' + name + '.jpg';
      ['webm', 'mp4'].forEach(function (ext) {
        var s = document.createElement('source');
        s.src = '/sky/' + name + '.' + ext; s.type = 'video/' + ext;
        v.append(s);
      });
      el.append(v);
    }
    return v;
  }

  function update() {
    clearTimeout(timer);
    var hour = currentHour();
    root.setAttribute('data-sky-phase', phase(hour));
    if (!document.querySelector('.zoen-sky')) return;
    var w = weights(hour);
    PHASES.forEach(function (name) {
      if (!w[name] && !layers[name]) return;
      var el = layer(name);
      // Real crossfade: the outgoing clip stays opaque underneath and the incoming one fades in on
      // top, so the plain page colour never shows through mid-blend.
      el.style.zIndex = name === w.to ? '2' : '1';
      el.style.opacity = name === w.to ? String(w[name]) : (name === w.from ? '1' : '0');
      var v = w[name] ? video(name, el) : el.querySelector('video');
      if (!v) return;
      if (w[name] && !document.hidden && !api.frozen) { var p = v.play(); if (p && p.catch) p.catch(function () {}); }
      else v.pause();
    });
    if (!document.hidden) timer = setTimeout(update, 60000);
  }

  /** Test hook: pause every visible clip at time t (seconds) and resolve once the frames are shown. */
  api.freeze = function (t) {
    api.frozen = true;
    var vids = [].slice.call(document.querySelectorAll('.zoen-sky video'));
    return Promise.all(vids.map(function (v) {
      v.pause();
      return new Promise(function (res) {
        var go = function () {
          var d = isFinite(v.duration) ? v.duration : 8;
          v.addEventListener('seeked', function () { res(); }, { once: true });
          v.currentTime = Math.min(t, d - 0.05);
        };
        if (v.readyState >= 1) go(); else v.addEventListener('loadedmetadata', go, { once: true });
      });
    }));
  };

  function mount() {
    if (!document.querySelector('.zoen-sky')) {
      var sky = document.createElement('div');
      sky.className = 'zoen-sky';
      sky.setAttribute('aria-hidden', 'true'); // purely decorative
      document.body.prepend(sky);
    }
    update();
  }

  document.addEventListener('visibilitychange', update);
  if (still.addEventListener) still.addEventListener('change', update);
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', mount);
  else mount();
  root.setAttribute('data-sky-phase', phase(currentHour()));
})();
