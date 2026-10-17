export const MESSAGE_SWIPE_THRESHOLD = 64;
export const MESSAGE_EDGE_ZONE = 24;
export type MessageSwipeIntent = 'reply' | 'thread';

/** Release semantics match the original iPhone editor, including a deliberate short flick. */
export function messageSwipeIntent(distance: number, velocity: number): MessageSwipeIntent | null {
  'worklet';
  if (!Number.isFinite(distance) || !Number.isFinite(velocity)) return null;
  const ahead = distance + velocity * 0.15;
  const crossed = Math.abs(distance) >= MESSAGE_SWIPE_THRESHOLD;
  const flick =
    Math.abs(distance) >= MESSAGE_SWIPE_THRESHOLD * 0.55 &&
    Math.abs(ahead) >= MESSAGE_SWIPE_THRESHOLD * 1.8 &&
    Math.sign(distance) === Math.sign(ahead);
  return crossed || flick ? (distance < 0 ? 'reply' : 'thread') : null;
}
export function messageSwipeOffset(distance: number): number {
  'worklet';
  const magnitude = Math.abs(distance);
  const shown =
    magnitude <= MESSAGE_SWIPE_THRESHOLD
      ? magnitude * 0.9
      : MESSAGE_SWIPE_THRESHOLD * 0.9 + (magnitude - MESSAGE_SWIPE_THRESHOLD) * 0.22;
  return Math.sign(distance) * Math.min(104, shown);
}

/** Fail before activation, so native vertical scrolling and the system back edge win. */
export function messageSwipeAdmission(
  startX: number,
  dx: number,
  dy: number,
  pointers = 1,
): 'wait' | 'fail' | 'activate' {
  'worklet';
  if (
    ![startX, dx, dy].every(Number.isFinite) ||
    pointers !== 1 ||
    startX < MESSAGE_EDGE_ZONE ||
    Math.abs(dy) > 12
  )
    return 'fail';
  return Math.abs(dx) > 12 && Math.abs(dx) > Math.abs(dy) * 1.8 ? 'activate' : 'wait';
}
