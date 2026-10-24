import type { ContactPreview, LocalMediaFile } from './types';

/** Optional platform parser only; native owns authorization and the held file lease. */
export function nativeContactReader(port: { readContactFile(request: string): Promise<string> }) {
  return async (file: LocalMediaFile): Promise<ContactPreview> => {
    const encoded = await port.readContactFile(
      JSON.stringify({
        uri: file.uri,
        itemId: file.itemId,
        version: file.version,
        sha256: file.info.sha256,
      }),
    );
    if (typeof encoded !== 'string' || encoded.length > 8192) throw new Error('contact-invalid');
    const raw: unknown = JSON.parse(encoded);
    const bytes = (text: string) => new TextEncoder().encode(text).length;
    const controls = (text: string) =>
      [...text].some((point) => {
        const code = point.charCodeAt(0);
        return code < 32 || (code >= 127 && code <= 159);
      });
    if (typeof raw !== 'object' || !raw || !('name' in raw) || !('phones' in raw))
      throw new Error('contact-invalid');
    const { name, phones } = raw;
    if (
      typeof name !== 'string' ||
      !name.trim() ||
      name.includes('\0') ||
      bytes(name) > 256 ||
      !Array.isArray(phones) ||
      phones.length > 10
    )
      throw new Error('contact-invalid');
    const values = phones.map((phone: unknown) => {
      if (typeof phone !== 'object' || !phone || !('label' in phone) || !('value' in phone))
        throw new Error('contact-invalid');
      const { label, value } = phone;
      if (
        typeof label !== 'string' ||
        typeof value !== 'string' ||
        !value.trim() ||
        bytes(label) > 64 ||
        bytes(value) > 80 ||
        controls(label + value)
      )
        throw new Error('contact-invalid');
      return Object.freeze({ label, value });
    });
    return Object.freeze({ name, phones: Object.freeze(values) });
  };
}
