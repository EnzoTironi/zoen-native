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

/** One focus epoch owns both its pending loader and its mounted document. */
export function frameClosures() {
  let active = true;
  const closers = new Set<() => void>();
  return {
    register(close: () => void): () => void {
      if (!active) {
        close();
        return () => {};
      }
      closers.add(close);
      return () => {
        closers.delete(close);
      };
    },
    close() {
      if (!active) return;
      active = false;
      const current = [...closers];
      closers.clear();
      let failed = false;
      let failure: unknown;
      for (const close of current) {
        try {
          close();
        } catch (error) {
          if (!failed) failure = error;
          failed = true;
        }
      }
      if (failed) throw failure;
    },
  };
}
