import type { ItemDetail, PageDto } from '../core/types';

export type PageImageReference = Readonly<{
  itemId: string;
  version: number;
  sha256: string;
}>;
const imageTypes = new Set(['image/png', 'image/jpeg', 'image/webp', 'image/gif', 'image/avif']);

/** Document-relative paths only. A Markdown URL is never a local SDK URI. */
export function pageImagePath(pagePath: string, source: string): string | null {
  if (!source || source.length > 4096 || /[\\\p{Cc}?#]/u.test(source) || source.startsWith('/'))
    return null;
  let decoded: string;
  try {
    decoded = decodeURIComponent(source).normalize('NFC');
  } catch {
    return null;
  }
  if (/[\\\p{Cc}%?#:]/u.test(decoded) || decoded.startsWith('/')) return null;
  const parent = pagePath ? pagePath.normalize('NFC').split('/').slice(0, -1) : [];
  if (
    pagePath &&
    pagePath
      .split('/')
      .some((part) => !part || part === '.' || part === '..' || /[\\\p{Cc}:]/u.test(part))
  )
    return null;
  for (const part of decoded.split('/')) {
    if (!part) return null;
    if (part === '.') continue;
    if (part === '..') {
      if (!parent.length) return null;
      parent.pop();
    } else parent.push(part);
  }
  return parent.length ? parent.join('/') : null;
}

/** Resolve exactly one signed File in this Space; never guess a duplicate path/version. */
export function pageImageReference(
  page: Pick<PageDto, 'space_id' | 'path'>,
  source: string,
  items: readonly ItemDetail[],
): PageImageReference | null {
  const path = pageImagePath(page.path, source);
  if (!path) return null;
  const matches = items.filter(
    (item) => item.space_id === page.space_id && item.path.normalize('NFC') === path,
  );
  const item = matches.length === 1 ? matches[0] : null;
  if (
    !item ||
    item.kind_id !== 'file' ||
    !item.file ||
    !imageTypes.has(item.file.mime) ||
    !Number.isSafeInteger(item.version) ||
    item.version < 1 ||
    !/^[a-f0-9]{64}$/i.test(item.file.sha256)
  )
    return null;
  return { itemId: item.id, version: item.version, sha256: item.file.sha256 };
}
