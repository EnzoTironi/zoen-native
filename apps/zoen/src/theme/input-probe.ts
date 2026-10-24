/** Temporary #440 seam. Disabled builds log nothing and retain no input history. */
export const THEME_INPUT_PROBE = process.env.EXPO_PUBLIC_ZOEN_THEME_INPUT_PROBE === '1';
const stages = [
  'screen-touch-start',
  'js-callback',
  'preference-request',
  'preference-committed',
] as const;
type Stage = (typeof stages)[number];
export function createThemeInputProbe(enabled: boolean, emit: (line: string) => void) {
  const counts = new Map<Stage, number>();
  return (stage: Stage) => {
    if (!enabled || !stages.includes(stage)) return;
    const count = (counts.get(stage) ?? 0) + 1;
    if (count > 8) return;
    counts.set(stage, count);
    try {
      emit(`[ZoenThemeInput] ${stage} count=${count}`);
    } catch {
      /* Diagnostics never block the actual preference change. */
    }
  };
}
export const reportThemeInput = createThemeInputProbe(THEME_INPUT_PROBE, (line) =>
  console.info(line),
);
