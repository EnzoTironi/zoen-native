// Permission-free local civil time; no timers run while the document is hidden.
export function skySample(hour: number): { rgb: number[]; stars: number } {
  const hours = [0, 6, 9, 12, 17, 20, 24];
  const colors = [[.20,.23,.46],[.84,.48,.53],[.38,.67,.87],[.27,.62,.90],[.94,.61,.30],[.51,.34,.63],[.20,.23,.46]];
  const night = [1,0,0,0,0,.4,1];
  const h = Number.isFinite(hour) ? ((hour % 24) + 24) % 24 : 12;
  const i = hours.findIndex((end, index) => index > 0 && h < end) - 1;
  const t = (h - hours[i]!) / (hours[i + 1]! - hours[i]!);
  const s = t * t * (3 - 2 * t);
  return { rgb: colors[i]!.map((v, c) => v + (colors[i + 1]![c]! - v) * s), stars: night[i]! + (night[i + 1]! - night[i]!) * s };
}

export function startLivingSky(): () => void {
  const dark = matchMedia('(prefers-color-scheme: dark)');
  const reduced = matchMedia('(prefers-reduced-motion: reduce)');
  let timer: ReturnType<typeof setTimeout> | undefined;
  function update() {
    clearTimeout(timer);
    if (document.hidden) return;
    const now = new Date();
    // Loopback-only preview override. Hosted production never reads it.
    const raw = ['localhost', '127.0.0.1', '[::1]'].includes(location.hostname) ? new URLSearchParams(location.search).get('skyHour') : null;
    const hour = raw !== null && Number.isFinite(Number(raw)) ? Number(raw) : now.getHours() + now.getMinutes() / 60 + now.getSeconds() / 3600;
    const { rgb, stars } = skySample(hour);
    const base = dark.matches ? [.035,.05,.08] : [.96,.97,.985];
    const color = rgb.map((v, i) => Math.round((base[i]! * .92 + v * .08) * 255));
    document.documentElement.style.setProperty('--zoen-sky', `rgb(${color.join(',')})`);
    document.documentElement.style.setProperty('--zoen-sky-base', `rgb(${base.map(v => Math.round(v * 255)).join(',')})`);
    document.documentElement.style.setProperty('--zoen-stars', String(stars * .025));
    timer = setTimeout(update, reduced.matches ? 60_000 : 1_000);
  }
  document.addEventListener('visibilitychange', update);
  dark.addEventListener('change', update);
  reduced.addEventListener('change', update);
  update();
  return () => {
    clearTimeout(timer);
    document.removeEventListener('visibilitychange', update);
    dark.removeEventListener('change', update);
    reduced.removeEventListener('change', update);
  };
}
