import type { Messages, FileDoc, VoiceMeta } from '../../../../packages/zoen-ui/web/messages';
import type { FileDto } from './generated/FileDto';

export interface BrowserMediaHost {
  readonly closed: AbortSignal;
  sendFile(
    space: string,
    file: {
      name: string;
      mime: string;
      bytes: Uint8Array;
      thumb?: Uint8Array | null;
      voice?: VoiceMeta;
    },
  ): Promise<{ itemId: string; messageId: string }>;
  file(
    itemId: string,
    version?: number,
  ): Promise<{
    itemId: string;
    version: number;
    doc: FileDoc;
    info: FileDto;
  }>;
  openFile(doc: FileDoc): Promise<Blob>;
}

let active: BrowserMediaHost | null = null;
let activeController: AbortController | null = null;

export function getBrowserMediaHost(): BrowserMediaHost {
  if (!active || active.closed.aborted) throw new Error('Vincule este navegador primeiro.');
  return active;
}

/** The account session owns media access; closing it invalidates every outstanding read. */
export function activateBrowserMediaHost(messages: Messages): () => void {
  const controller = new AbortController();
  activeController?.abort();
  activeController = controller;
  const host: BrowserMediaHost = {
    closed: controller.signal,
    sendFile: (space, file) => messages.sendFile(space, file),
    file: async (itemId, version) => {
      const result = await messages.fileManifest(itemId, version);
      return { ...result, info: result.info as FileDto };
    },
    openFile: (doc) => messages.openFile(doc),
  };
  active = host;
  return () => {
    controller.abort();
    if (active === host) {
      active = null;
      activeController = null;
    }
  };
}
