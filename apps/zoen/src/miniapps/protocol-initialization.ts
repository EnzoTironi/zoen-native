import { t } from '../i18n/index.ts';

export const MINIAPP_REQUEST_TIMEOUT = 20_000;

/** The bridge event must arrive within the existing App RPC deadline. Disposal
 * resolves the signal separately; the caller rechecks its own active lifetime. */
export async function waitForMiniappInitialization(initialization: Promise<void>) {
  let cancel = () => {};
  const expired = new Promise<never>((_resolve, reject) => {
    const timer = setTimeout(
      () => reject(new Error(t('miniapps.theInterfaceHasNotInitializedYet'))),
      MINIAPP_REQUEST_TIMEOUT,
    );
    cancel = () => clearTimeout(timer);
  });
  try {
    await Promise.race([initialization, expired]);
  } finally {
    cancel();
  }
}
