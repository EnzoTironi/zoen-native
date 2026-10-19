export type PushWake = { spaceId: string | null; eventSequence: string | null };

const object = (value: unknown): value is Record<string, unknown> =>
  typeof value === 'object' && value !== null && !Array.isArray(value);

/** An untrusted content-free sync hint. It never authorizes a route or mutation. */
export function decodeWake(payload: unknown, transport: 'apns' | 'fcm' = 'apns'): PushWake | null {
  try {
    const text = typeof payload === 'string' ? payload : JSON.stringify(payload);
    if (typeof text !== 'string' || new TextEncoder().encode(text).length > 4096) return null;
    const input: unknown = typeof payload === 'string' ? JSON.parse(payload) : payload;
    // Native FCM sends precisely message.data={wake:"1"}, without notification text.
    if (transport === 'fcm')
      return object(input) && Object.keys(input).join(',') === 'wake' && input.wake === '1'
        ? { spaceId: null, eventSequence: null }
        : null;
    if (
      !object(input) ||
      !object(input.aps) ||
      Object.keys(input.aps).join(',') !== 'content-available' ||
      input.aps['content-available'] !== 1
    )
      return null;
    if (Object.keys(input).length === 1) return { spaceId: null, eventSequence: null };
    if (
      Object.keys(input).sort().join(',') !== 'aps,zoen' ||
      !object(input.zoen) ||
      Object.keys(input.zoen).sort().join(',') !== 'event_seq,space_id,v' ||
      input.zoen.v !== 1 ||
      typeof input.zoen.space_id !== 'string' ||
      !/^[A-Za-z0-9_-]{1,128}$/.test(input.zoen.space_id) ||
      typeof input.zoen.event_seq !== 'string' ||
      !/^[1-9][0-9]{0,19}$/.test(input.zoen.event_seq) ||
      BigInt(input.zoen.event_seq) > 18446744073709551615n
    )
      return null;
    return { spaceId: input.zoen.space_id, eventSequence: input.zoen.event_seq };
  } catch {
    return null;
  }
}
