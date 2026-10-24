export type Scope = { artifactId: string; actorId: string; actorKind: 'human' | 'agent' };
export type Head = { version: string; sha256: string };
export type Reply = { metadata: string; bytes: Uint8Array };
export type DeckcraftBindings = {
  zoenHostRequest: (envelope: string, bytes: Uint8Array) => Promise<Reply>;
  zoenHostOnChange: (callback?: (status: string) => void) => void;
  zoenHostRetire: () => void;
};
export const token = (v: unknown): v is string =>
  typeof v === 'string' && /^[A-Za-z0-9_-]{1,128}$/.test(v);
export const digest = (v: unknown): v is string =>
  typeof v === 'string' && /^[a-f0-9]{64}$/.test(v);
export const positive = (v: unknown): v is number =>
  typeof v === 'number' && Number.isSafeInteger(v) && v >= 1 && v <= 0xffffffff;
export const headEqual = (a: Head, b: Head) => a.version === b.version && a.sha256 === b.sha256;
export function record(v: unknown): Record<string, unknown> {
  if (!v || typeof v !== 'object' || Array.isArray(v))
    throw new Error('Invalid document response.');
  return v as Record<string, unknown>;
}
export function head(v: unknown): Head {
  const value = record(v);
  if (
    typeof value.version !== 'string' ||
    !/^[1-9][0-9]{0,9}$/.test(value.version) ||
    !positive(Number(value.version)) ||
    !digest(value.sha256)
  )
    throw new Error('Invalid accepted document head.');
  return { version: value.version, sha256: value.sha256 };
}
export function scope(v: unknown, item: string): Scope {
  const value = record(v);
  if (
    value.artifactId !== item ||
    !token(value.actorId) ||
    !['human', 'agent'].includes(String(value.actorKind))
  )
    throw new Error('Invalid document binding.');
  return {
    artifactId: item,
    actorId: value.actorId,
    actorKind: value.actorKind as Scope['actorKind'],
  };
}
export function scopeEqual(a: Scope, b: Scope) {
  return a.artifactId === b.artifactId && a.actorId === b.actorId && a.actorKind === b.actorKind;
}
