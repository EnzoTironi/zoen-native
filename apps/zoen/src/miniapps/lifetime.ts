import { t } from '../i18n/index.ts';
/** A closed frame never regains authority when a new frame is initialized. */
export function frameAuthority(checkOwner: () => void) {
  let active = true;
  return {
    check: () => {
      if (!active) throw new Error(t('miniapps.thisInterfaceHasClosed'));
      checkOwner();
    },
    close: () => {
      active = false;
    },
  };
}
