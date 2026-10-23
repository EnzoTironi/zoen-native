import type { ClientStore } from '../core/store';
import type { SiteScope } from '../core/generated/SiteScope';
import type { SourceVersion } from '../core/generated/SourceVersion';
import { documentScope } from '../documents/document-authority.ts';
import { homeOwnerFor, homeSpacesFor } from '../home/access.ts';

type SourceStore = Pick<ClientStore, 'getSnapshot' | 'sessionEpoch'>;
export function siteScope(store: SourceStore, spaceId: string): SiteScope | null {
  const state = store.getSnapshot();
  const owner = homeOwnerFor(state);
  return owner?.mode === 'account' && homeSpacesFor(state).has(spaceId)
    ? { identity_id: owner.identityId, device_id: owner.deviceId, space_id: spaceId }
    : null;
}
export function siteAuthority(store: SourceStore, scope: SiteScope): string | null {
  const current = siteScope(store, scope.space_id);
  return current?.identity_id === scope.identity_id && current.device_id === scope.device_id
    ? documentScope(store.getSnapshot(), store.sessionEpoch)
    : null;
}
export function sameSource(a: SourceVersion, b: SourceVersion): boolean {
  return (
    a.space_id === b.space_id &&
    a.item_id === b.item_id &&
    a.version === b.version &&
    a.sha256 === b.sha256 &&
    a.byte_length === b.byte_length
  );
}
export function validSiteText(html: string): boolean {
  const size = new TextEncoder().encode(html).length;
  return size > 0 && size <= 2 * 1024 * 1024 && !html.includes('\0');
}
export function siteFilename(title: string): string {
  const base = title
    .trim()
    .replace(/[\\/\p{Cc}]/gu, ' ')
    .trim();
  return `${base.replace(/\.html$/iu, '') || 'site'}.html`;
}
export function emptySite(title: string): string {
  const escaped = title
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;')
    .replaceAll("'", '&#39;');
  return `<!doctype html>\n<html lang="en">\n<head>\n<meta charset="utf-8">\n<meta name="viewport" content="width=device-width,initial-scale=1">\n<title>${escaped}</title>\n</head>\n<body>\n<h1>${escaped}</h1>\n</body>\n</html>\n`;
}
