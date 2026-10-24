/** MIME declares the contact format; extensions/names never grant parsing authority. */
export function isContactFileMime(mime: string): boolean {
  return ['text/vcard', 'text/x-vcard'].includes(mime.split(';', 1)[0]!.trim().toLowerCase());
}
