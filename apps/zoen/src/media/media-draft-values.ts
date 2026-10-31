/** Bounded JSON values shared by private media contracts. No native dependencies. */
export const MAX_MEDIA_DRAFT_CONTENT_BYTES = 512 * 1024;
export class MediaDraftFailure extends Error {
  readonly kind: 'closed' | 'notLoaded' | 'invalid' | 'busy' | 'attempted' | 'conflict';
  constructor(kind: MediaDraftFailure['kind'], cause?: unknown) {
    super(cause instanceof Error ? cause.message : `Media draft ${kind}`, { cause });
    this.name = 'MediaDraftFailure';
    this.kind = kind;
  }
}
export function invalid(): never {
  throw new MediaDraftFailure('invalid');
}
export const bytes = (text: string) => {
  let count = 0;
  for (const letter of text) {
    const point = letter.codePointAt(0)!;
    count += point < 128 ? 1 : point < 2048 ? 2 : point < 65536 ? 3 : 4;
  }
  return count;
};
export const id = (value: unknown): value is string =>
  typeof value === 'string' && /^[A-Za-z0-9_-]{1,128}$/.test(value);
export const boundedText = (value: unknown, limit: number): value is string =>
  typeof value === 'string' && !value.includes('\0') && bytes(value) <= limit;
export const hasControl = (value: string) =>
  [...value].some((char) => char.charCodeAt(0) < 32 || char.charCodeAt(0) === 127);
export const target = (value: unknown): value is string | null =>
  value === null || (boundedText(value, 256) && value.length > 0);
export const object = (value: unknown): Record<string, unknown> => {
  if (!value || typeof value !== 'object' || Array.isArray(value)) return invalid();
  return value as Record<string, unknown>;
};
export function keys(value: Record<string, unknown>, expected: string[]) {
  if (Object.keys(value).sort().join(',') !== expected.sort().join(',')) invalid();
}
export function freeze<T>(value: T): T {
  if (value && typeof value === 'object') {
    for (const child of Object.values(value)) freeze(child);
    Object.freeze(value);
  }
  return value;
}
/** Opaque editor data remains JSON, never an SDK object, cache path or file lease. */
export function editorJson(value: unknown, depth = 0, seen = new Set<unknown>()): unknown {
  if (depth > 64) invalid();
  if (value === null || typeof value === 'boolean') return value;
  if (typeof value === 'number') return Number.isFinite(value) ? value : invalid();
  if (typeof value === 'string') {
    if (/^(?:file:|content:|blob:|ph:|assets-library:|\/private\/|\/var\/|\/Users\/)/i.test(value))
      invalid();
    return value;
  }
  if (!value || typeof value !== 'object' || seen.has(value)) return invalid();
  const proto = Object.getPrototypeOf(value);
  if (!Array.isArray(value) && proto !== Object.prototype && proto !== null) invalid();
  seen.add(value);
  let result: unknown;
  if (Array.isArray(value)) result = value.map((child) => editorJson(child, depth + 1, seen));
  else {
    const record: Record<string, unknown> = Object.create(null) as Record<string, unknown>;
    for (const [key, child] of Object.entries(value).sort(([left], [right]) =>
      left.localeCompare(right),
    )) {
      if (/(?:^|_)(?:uri|url|path|lease)(?:$|_)|(?:Uri|URL|Path|Lease)$/.test(key)) invalid();
      record[key] = editorJson(child, depth + 1, seen);
    }
    result = record;
  }
  seen.delete(value);
  return result;
}
