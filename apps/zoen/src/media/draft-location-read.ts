import { parseLocationGeoJSON } from './location-file.ts';
import type { StaticLocation } from './types';
/** A display-only read of the journal's already materialized, held copy. */
export async function readDraftLocation(
  source: { uri: string; sha256: string },
  port: {
    size(uri: string): number;
    bytes(uri: string, maximum: number): Promise<Uint8Array>;
    sha256(text: string): Promise<string>;
  },
  current: () => boolean,
): Promise<StaticLocation | null> {
  if (!current()) return null;
  const size = port.size(source.uri);
  if (
    !Number.isSafeInteger(size) ||
    size <= 0 ||
    size > 4096 ||
    !/^[a-f0-9]{64}$/.test(source.sha256)
  )
    throw Error('location-invalid');
  // Read one extra byte to detect growth, without an unbounded read after size preflight.
  const bytes = await port.bytes(source.uri, 4097);
  if (!current()) return null;
  if (bytes.length !== size || bytes.length > 4096) throw Error('location-invalid');
  const text = new TextDecoder('utf-8', { fatal: true }).decode(bytes);
  const hash = await port.sha256(text);
  if (!current()) return null;
  if (hash !== source.sha256) throw Error('location-changed');
  return parseLocationGeoJSON(text);
}
