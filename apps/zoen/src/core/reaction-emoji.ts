import emojiRegex from 'emoji-regex';

/** Keep the core's annotation bound, including multi-code-point emoji sequences. */
export const MAX_REACTION_EMOJI_BYTES = 32;

export function normalizeReactionEmoji(input: string): string | null {
  const value = input.trim();
  // A valid UTF-8 sequence cannot have fewer bytes than UTF-16 code units.
  if (!value || value.length > MAX_REACTION_EMOJI_BYTES) return null;
  const match = emojiRegex().exec(value);
  if (match?.index !== 0 || match[0] !== value) return null;
  let bytes = 0;
  for (const scalar of value) {
    const point = scalar.codePointAt(0)!;
    bytes += point <= 0x7f ? 1 : point <= 0x7ff ? 2 : point <= 0xffff ? 3 : 4;
  }
  return bytes <= MAX_REACTION_EMOJI_BYTES ? value : null;
}
