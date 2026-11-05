/** One local entity, never a URL or a request for public/global discovery. */
export function messageHashtag(value: string): string | null {
  if (!/^#[\p{L}\p{M}\p{N}_]+$/u.test(value) || !/[\p{L}\p{N}]/u.test(value)) return null;
  return new TextEncoder().encode(value).byteLength <= 512 ? value.normalize('NFC') : null;
}

export function messageHashtagLink(value: string): string | null {
  if (!value.startsWith('zoen-hashtag:')) return null;
  try {
    return messageHashtag(decodeURIComponent(value.slice('zoen-hashtag:'.length)));
  } catch {
    return null;
  }
}

export function messageMayContainHashtags(value: string): boolean {
  return /(?:^|[\s([{<"',;!?—–])#[\p{L}\p{M}\p{N}_]+/u.test(value);
}
