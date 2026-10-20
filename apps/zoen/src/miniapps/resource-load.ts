import type { AppResourceDto } from '../core/generated/AppResourceDto';
import { t } from '../i18n/index.ts';
import { verifyMiniappResource, type VerifiedMiniappResource } from './sandbox.ts';

/** Neither an old resource response nor a late native digest can publish into a new owner. */
export async function readVerifiedMiniappResource(
  read: () => Promise<AppResourceDto>,
  appId: string,
  uri: string,
  check: () => void,
): Promise<VerifiedMiniappResource> {
  check();
  const resource = await read();
  check();
  if (resource.uri !== uri) throw new Error(t('miniapps.thisResourceDoesNotBelongToTheOpen'));
  const verified = await verifyMiniappResource(resource, appId);
  check();
  return verified;
}
