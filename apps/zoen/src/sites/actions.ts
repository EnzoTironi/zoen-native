import { sha256 } from '@noble/hashes/sha2.js';
import { bytesToHex } from '@noble/hashes/utils.js';
import type { ClientStore } from '../core/store';
import type { SiteScope } from '../core/generated/SiteScope';
import type { SiteSource } from '../core/generated/SiteSource';
import type { SourceVersion } from '../core/generated/SourceVersion';
import { t } from '../i18n/index.ts';
import { siteAuthority, validSiteText } from './source.ts';

type SiteStore = Pick<ClientStore, 'driver' | 'getSnapshot' | 'sessionEpoch' | 'request'>;
/** The checked write stays in Rust. This delivery fence only prevents an old
 * operation from replacing the editor after focus, account, or membership loss. */
export async function saveObservedSite(
  store: SiteStore,
  scope: SiteScope,
  authority: string,
  source: SourceVersion,
  html: string,
  active: () => boolean,
): Promise<SiteSource | null> {
  const current = () => active() && siteAuthority(store, scope) === authority;
  if (!current()) return null;
  if (!validSiteText(html)) throw new Error(t('sites.tooLarge'));
  const next = await store.request({ op: 'site_save', scope, source, html });
  if (!current()) return null;
  const bytes = new TextEncoder().encode(html);
  if (
    next.source.item_id !== source.item_id ||
    next.source.space_id !== source.space_id ||
    next.source.version !== source.version + 1 ||
    next.current_version !== next.source.version ||
    next.source.byte_length !== bytes.length ||
    next.source.sha256 !== bytesToHex(sha256(bytes)) ||
    next.html !== html
  )
    throw new Error(t('sites.failedSave'));
  return next;
}
