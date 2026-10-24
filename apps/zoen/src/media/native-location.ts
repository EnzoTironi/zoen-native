import type { LocalMediaFile, StaticLocation } from './types';

/** Validate native metadata; bytes and current file authority remain native-owned. */
export function nativeLocationPoint(value: unknown): StaticLocation {
  if (typeof value !== 'object' || !value || Array.isArray(value))
    throw new Error('location-invalid');
  const raw = value as Record<string, unknown>;
  if (
    Object.keys(raw).some((key) => !['longitude', 'latitude', 'label'].includes(key)) ||
    typeof raw.longitude !== 'number' ||
    !Number.isFinite(raw.longitude) ||
    raw.longitude < -180 ||
    raw.longitude > 180 ||
    typeof raw.latitude !== 'number' ||
    !Number.isFinite(raw.latitude) ||
    raw.latitude < -90 ||
    raw.latitude > 90 ||
    (raw.label !== undefined &&
      (typeof raw.label !== 'string' ||
        !raw.label.trim() ||
        new TextEncoder().encode(raw.label).length > 256 ||
        /[\u0000-\u001f\u007f-\u009f]/u.test(raw.label)))
  )
    throw new Error('location-invalid');
  return Object.freeze({
    longitude: raw.longitude,
    latitude: raw.latitude,
    ...(typeof raw.label === 'string' ? { label: raw.label } : {}),
  });
}

export function nativeLocationReader(port: { readLocationFile(request: string): Promise<string> }) {
  return async (file: LocalMediaFile): Promise<StaticLocation> => {
    const encoded = await port.readLocationFile(
      JSON.stringify({
        uri: file.uri,
        itemId: file.itemId,
        version: file.version,
        sha256: file.info.sha256,
      }),
    );
    if (typeof encoded !== 'string' || encoded.length > 4096) throw new Error('location-invalid');
    return nativeLocationPoint(JSON.parse(encoded));
  };
}

export function nativeLocationSearch(port: {
  searchLocations(request: string): Promise<string>;
  cancelLocationSearch(binding: string): Promise<void>;
}) {
  return {
    async searchLocations(binding: string, query: string): Promise<readonly StaticLocation[]> {
      if (
        !binding ||
        new TextEncoder().encode(binding).length > 128 ||
        /[\u0000-\u001f\u007f-\u009f]/u.test(binding) ||
        !query.trim() ||
        new TextEncoder().encode(query).length > 256 ||
        /[\u0000-\u001f\u007f-\u009f]/u.test(query)
      )
        throw new Error('location-invalid');
      const encoded = await port.searchLocations(JSON.stringify({ binding, query }));
      if (typeof encoded !== 'string' || encoded.length > 16384)
        throw new Error('location-invalid');
      const raw: unknown = JSON.parse(encoded);
      if (!Array.isArray(raw) || raw.length > 20) throw new Error('location-invalid');
      return Object.freeze(raw.map(nativeLocationPoint));
    },
    cancelLocationSearch(binding: string) {
      // The UI retires its binding synchronously before this best-effort native
      // cancellation; a rejection cannot permit an old callback or resend.
      void port.cancelLocationSearch(binding).catch(() => {});
    },
  };
}
