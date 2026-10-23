import type { ClientStore } from '../core/store';
import type { SiteScope } from '../core/generated/SiteScope';
import type { SourceVersion } from '../core/generated/SourceVersion';
import type { SiteReview } from '../core/generated/SiteReview';
import type { Receipt } from '../core/generated/Receipt';
import { t } from '../i18n/index.ts';
import { sameSource, siteAuthority } from './source.ts';

type PublicationStore = Pick<ClientStore, 'driver' | 'getSnapshot' | 'sessionEpoch' | 'request'>;
export function publicSiteURL(review: SiteReview): string {
  const origin = review.configuration.public_origin;
  const url = new URL(origin);
  const local =
    url.hostname === 'localhost' ||
    url.hostname === '[::1]' ||
    /^127\.\d+\.\d+\.\d+$/u.test(url.hostname);
  if (
    review.mutation.public_origin !== origin ||
    url.origin !== origin ||
    url.pathname !== '/' ||
    !(url.protocol === 'https:' || (url.protocol === 'http:' && local)) ||
    url.search ||
    url.hash ||
    url.username ||
    url.password ||
    !/^site_[0-9a-f]{32}$/u.test(review.mutation.site_id)
  )
    throw new Error(t('sites.changedConsent'));
  return `${origin}/s/${review.mutation.site_id}`;
}

/** Review is read-only. Cancellation, a changed source, or a stale native dialog
 * never calls the signing/publication operation. Retry identity belongs to Core. */
export async function publishObservedSite(
  store: PublicationStore,
  scope: SiteScope,
  authority: string,
  source: SourceVersion,
  unpublish: boolean,
  active: () => boolean,
  confirm: (review: SiteReview, url: string) => Promise<boolean>,
): Promise<Receipt | null> {
  const current = () => active() && siteAuthority(store, scope) === authority;
  if (!current()) return null;
  const review = await store.driver.request({ op: 'site_review', scope, source, unpublish });
  if (!current()) return null;
  if (
    review.scope.identity_id !== scope.identity_id ||
    review.scope.device_id !== scope.device_id ||
    review.scope.space_id !== scope.space_id ||
    !sameSource(review.source, source) ||
    (review.mutation.change.kind === 'publish' &&
      !sameSource(review.mutation.change.source, source))
  )
    throw new Error(t('sites.changedConsent'));
  const encoded = JSON.stringify(review);
  const url = publicSiteURL(review);
  if (!(await confirm(review, url)) || !current()) return null;
  // The dialog callback cannot change the reviewed object after presentation.
  if (JSON.stringify(review) !== encoded) throw new Error(t('sites.changedConsent'));
  const receipt = await store.request({ op: 'site_publish', review });
  if (!current()) return null;
  if (
    receipt.owner_id !== scope.identity_id ||
    receipt.site_id !== review.mutation.site_id ||
    receipt.request_id !== review.mutation.request_id ||
    receipt.previous_head !== review.mutation.expected_head ||
    receipt.active !== (review.mutation.change.kind === 'publish') ||
    !/^[0-9a-f]{64}$/u.test(receipt.head) ||
    (!receipt.active && receipt.source !== null) ||
    (receipt.active && (!receipt.source || !sameSource(receipt.source, source)))
  )
    throw new Error(t('sites.publicationFailed'));
  return receipt;
}
