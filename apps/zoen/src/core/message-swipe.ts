export const MESSAGE_EDGE_ZONE = 24;
export type MessageSwipeIntent = 'reply' | 'thread';

export function messageSwipeThreshold(mine: boolean): number {
  'worklet';
  return mine ? 60 : 45;
}

/** The source prepares one threshold impact per gesture, including retreat and reversal. */
export function messageSwipeFeedback(
  played: boolean,
  distance: number | null,
  reducedMotion: boolean,
  mine: boolean,
): { played: boolean; tick: boolean } {
  'worklet';
  const crossed =
    distance !== null &&
    Number.isFinite(distance) &&
    Math.abs(distance) >= messageSwipeThreshold(mine);
  return { played: played || crossed, tick: !reducedMotion && crossed && !played };
}

/** Commit strictly beyond the raw drag threshold; a velocity-only flick cannot send an intent. */
export function messageSwipeIntent(distance: number, mine: boolean): MessageSwipeIntent | null {
  'worklet';
  return Number.isFinite(distance) && Math.abs(distance) > messageSwipeThreshold(mine)
    ? distance < 0
      ? 'reply'
      : 'thread'
    : null;
}

/** Telegram's rational band; the positive direction is Zoen's mirrored thread extension. */
export function messageSwipeOffset(distance: number, mine: boolean): number {
  'worklet';
  if (!Number.isFinite(distance)) return 0;
  const magnitude = Math.abs(distance),
    start = messageSwipeThreshold(mine);
  const shown =
    magnitude < start ? magnitude : start + (1 - 1 / (((magnitude - start) * 0.4) / 100 + 1)) * 100;
  return Math.sign(distance) * Math.min(180, shown);
}

/** Fail before activation, preserving native vertical scrolling and the system back edge. */
export function messageSwipeAdmission(
  startX: number,
  dx: number,
  dy: number,
  pointers = 1,
): 'wait' | 'fail' | 'activate' {
  'worklet';
  if (![startX, dx, dy].every(Number.isFinite) || pointers !== 1 || startX < MESSAGE_EDGE_ZONE)
    return 'fail';
  if (Math.abs(dy) > 2 && Math.abs(dy) > Math.abs(dx) * 2) return 'fail';
  return Math.abs(dx) > 2 && Math.abs(dy) * 2 < Math.abs(dx) ? 'activate' : 'wait';
}
