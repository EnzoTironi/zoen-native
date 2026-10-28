/** Candidate selection only. The maintained renderer owns syntax and escape decoding. */
export function messageMayContainMarkdown(text: string, nativeMediaDisabled = false): boolean {
  return (
    (nativeMediaDisabled || !messageMarkdownMayLoadMedia(text)) &&
    (/[*_`~[\]|]|(?:^|\n) {0,3}(?:#{1,6}\s|>\s?|[-+]\s|\d{1,9}[.)]\s)/u.test(text) ||
      text.includes('\\') ||
      /https?:\/\/|www\./iu.test(text) ||
      (text.includes('@') && text.includes('.')))
  );
}

/** Rendered content cannot choose an executable/local-file URL scheme. */
export function messageMarkdownLink(value: string): string | null {
  if (value !== value.trim()) return null;
  for (const character of value) {
    const code = character.charCodeAt(0);
    if (code <= 32 || code === 127) return null;
  }
  try {
    const url = new URL(value);
    if (!['https:', 'http:', 'mailto:', 'tel:', 'zoen:'].includes(url.protocol)) return null;
    if (url.username || url.password) return null;
    if ((url.protocol === 'https:' || url.protocol === 'http:') && !url.hostname) return null;
    return url.href;
  } catch {
    return null;
  }
}

/** The installed parser can fetch image nodes and promote HTML video blocks.
 * Platforms without qualified native admission keep possible media markup as exact plain text.
 * This conservative fallback intentionally includes escaped/code examples. */
export function messageMarkdownMayLoadMedia(text: string): boolean {
  return text.includes('![') || text.includes('<');
}
