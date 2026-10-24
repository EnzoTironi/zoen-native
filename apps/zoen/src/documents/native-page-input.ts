import type { PageInputOpen, PageInputPort, PageInputStatus } from './page-input';

interface NativePageInputModule {
  readonly pageInputVersion?: number;
  pageInputSession(): Promise<string>;
  loadPageInput(session: string, itemId: string): Promise<string>;
  reservePageInput(session: string, itemId: string): Promise<string>;
  openPageInput(request: string): Promise<string>;
  pageInputStatus(session: string, id: string): Promise<string>;
  sealPageInput(session: string, id: string): Promise<string>;
  applyPageInput(session: string, id: string): Promise<string>;
  discardPageInput(session: string, id: string): Promise<string>;
  closePageInput(session: string, id: string): Promise<void>;
  addListener(
    event: 'onPageInputStatus',
    listener: (event: { json: string }) => void,
  ): { remove(): void };
}

export function createNativePageInputPort(
  native: NativePageInputModule | null,
): PageInputPort | undefined {
  if (
    !native ||
    [
      native.pageInputSession,
      native.loadPageInput,
      native.reservePageInput,
      native.openPageInput,
      native.pageInputStatus,
      native.sealPageInput,
      native.applyPageInput,
      native.discardPageInput,
      native.closePageInput,
    ].some((method) => typeof method !== 'function')
  )
    return undefined;
  return {
    supportsWholeDocument: native.pageInputVersion === 2,
    session: () => native.pageInputSession(),
    load: async (session, itemId) => JSON.parse(await native.loadPageInput(session, itemId)),
    reserve: (session, itemId) => native.reservePageInput(session, itemId),
    open: async (request: PageInputOpen) =>
      JSON.parse(await native.openPageInput(JSON.stringify(request))),
    status: async (session, id) => JSON.parse(await native.pageInputStatus(session, id)),
    seal: async (session, id) => JSON.parse(await native.sealPageInput(session, id)),
    apply: async (session, id) => JSON.parse(await native.applyPageInput(session, id)),
    discard: async (session, id) => JSON.parse(await native.discardPageInput(session, id)),
    close: (session, id) => native.closePageInput(session, id),
    subscribe(listener) {
      const subscription = native.addListener('onPageInputStatus', ({ json }) => {
        listener(JSON.parse(json) as PageInputStatus);
      });
      return () => subscription.remove();
    },
  };
}
