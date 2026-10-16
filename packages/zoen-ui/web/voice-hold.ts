/** Native Composer drag thresholds; terminal decisions never send twice. */
export function voiceHold(x: number, y: number, actions: { send: () => void; cancel: () => void }) {
  let phase: 'holding' | 'locked' | 'ended' = 'holding';
  return {
    move(px: number, py: number) {
      if (phase !== 'holding') return phase;
      if (px - x < -110) { phase = 'ended'; actions.cancel(); }
      else if (py - y < -80) phase = 'locked';
      return phase;
    },
    release() { if (phase === 'holding') { phase = 'ended'; actions.send(); } },
    cancel() { if (phase !== 'ended') { phase = 'ended'; actions.cancel(); } },
  };
}
