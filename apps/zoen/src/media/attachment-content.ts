export const MAX_ARTICLE_BYTES = 64 * 1024;
const textBytes = (text: string) => new TextEncoder().encode(text).length;
export function articleText(text: string): string {
  if (typeof text !== 'string' || text.includes('\0') || textBytes(text) > MAX_ARTICLE_BYTES)
    throw new Error('article-limit');
  return text;
}
export function staticLocation(latitude: number, longitude: number): string {
  if (
    !Number.isFinite(latitude) ||
    !Number.isFinite(longitude) ||
    Math.abs(latitude) > 90 ||
    Math.abs(longitude) > 180
  )
    throw new Error('location-invalid');
  return `https://maps.apple.com/?ll=${latitude},${longitude}`;
}
export function appendAttachmentText(text: string, addition: string): string {
  return articleText(text ? `${text}\n\n${addition}` : addition);
}
export function pollArguments(
  question: string,
  options: readonly string[],
): { question: string; options: string[] } {
  const checked = (value: string, limit: number) => {
    if (
      typeof value !== 'string' ||
      !value.trim() ||
      value.includes('\0') ||
      textBytes(value) > limit
    )
      throw new Error('poll-invalid');
    return value.trim();
  };
  if (options.length < 2 || options.length > 10) throw new Error('poll-invalid');
  const result = options.map((option) => checked(option, 200));
  if (new Set(result).size !== result.length) throw new Error('poll-invalid');
  return { question: checked(question, 300), options: result };
}
export type SharedContact = Readonly<{ name: string; phones: readonly string[] }>;
export function sharedContact(name: string, phones: readonly string[]): SharedContact {
  if (
    typeof name !== 'string' ||
    !name.trim() ||
    textBytes(name) > 256 ||
    name.includes('\0') ||
    phones.length > 10
  )
    throw new Error('contact-invalid');
  const numbers = phones.map((number) => {
    if (
      typeof number !== 'string' ||
      !number.trim() ||
      textBytes(number) > 80 ||
      [...number].some((point) => point.charCodeAt(0) < 32 || point.charCodeAt(0) === 127)
    )
      throw new Error('contact-invalid');
    return number.trim();
  });
  return Object.freeze({ name: name.trim(), phones: Object.freeze([...new Set(numbers)]) });
}
/** Only the reviewed name and phone fields are exported; never an entire address-book record. */
export function contactVCard(contact: SharedContact): string {
  const value = sharedContact(contact.name, contact.phones);
  const escape = (text: string) =>
    text
      .replaceAll('\\', '\\\\')
      .replace(/\r\n|\r|\n/g, '\\n')
      .replaceAll(';', '\\;')
      .replaceAll(',', '\\,');
  // Fold at 75 UTF-8 octets without cutting a code point. Continuation space counts.
  const fold = (line: string) => {
    const lines: string[] = [];
    let current = '';
    for (const point of line) {
      if (textBytes(current + point) > 75) {
        lines.push(current);
        current = ' ';
      }
      current += point;
    }
    lines.push(current);
    return lines.join('\r\n');
  };
  return (
    [
      'BEGIN:VCARD',
      'VERSION:3.0',
      `FN:${escape(value.name)}`,
      `N:;${escape(value.name)};;;`,
      ...value.phones.map((phone) => `TEL:${escape(phone)}`),
      'END:VCARD',
    ]
      .map(fold)
      .join('\r\n') + '\r\n'
  );
}
