/** Candidate selection only. The maintained Markdown renderer owns parsing. */
export function messageMayContainMarkdown(text: string): boolean {
  return (
    !messageMarkdownMayLoadMedia(text) &&
    /[*_`~[\]|]|(?:^|\n) {0,3}(?:#{1,6}\s|>\s?|[-+]\s|\d{1,9}[.)]\s)/u.test(text)
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
 * Keep possible media markup as exact plain text until native image admission exists.
 * This conservative candidate check intentionally includes escaped/code examples. */
export function messageMarkdownMayLoadMedia(text: string): boolean {
  return text.includes('![') || text.includes('<');
}
