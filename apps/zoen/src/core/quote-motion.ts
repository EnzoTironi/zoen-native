// Telegram iOS f1dd7a2: ListView 0.4s; UIKitUtils iOS26 spring m=1,k=555.027,c=47.118.
// Public critical-spring evaluation replaces upstream's private CA valueAt.
export function quoteMotionCurve(t: number) {
  'worklet';
  const w = Math.sqrt(555.027) * 0.5;
  const value = (x: number) => 1 - (1 + w * x) * Math.exp(-w * x);
  return value(Math.max(0, Math.min(1, t))) / value(1);
}
