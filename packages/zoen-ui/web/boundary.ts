export function object(value: unknown): Record<string, unknown> {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) {
    throw new Error('Invalid browser core record.');
  }
  return { ...value };
}

export function string(value: unknown): string {
  if (typeof value !== 'string') throw new Error('Invalid browser core text.');
  return value;
}

export function integer(value: unknown): number {
  if (typeof value !== 'number' || !Number.isSafeInteger(value) || value < 0) {
    throw new Error('Invalid browser core number.');
  }
  return value;
}

export function boolean(value: unknown): boolean {
  if (typeof value !== 'boolean') throw new Error('Invalid browser core flag.');
  return value;
}

export function array(value: unknown): unknown[] {
  if (!Array.isArray(value)) throw new Error('Invalid browser core list.');
  return value;
}

export function bytes(value: unknown): Uint8Array<ArrayBuffer> {
  if (value instanceof Uint8Array) return Uint8Array.from(value);
  return Uint8Array.from(array(value).map((entry) => {
    const n = integer(entry);
    if (n > 255) throw new Error('Invalid byte.');
    return n;
  }));
}

export function message(error: unknown): string {
  return error instanceof Error ? error.message : typeof error === 'string' ? error : 'The operation failed.';
}

export function publicKey(value: unknown): string {
  const key = string(value);
  if (!/^[0-9a-f]{64}$/.test(key)) throw new Error('Invalid device public key.');
  return key;
}

export function hex(value: Uint8Array): string {
  return Array.from(value, (byte) => byte.toString(16).padStart(2, '0')).join('');
}
