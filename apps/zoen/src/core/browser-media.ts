import type { Messages, FileDoc, VoiceMeta } from '../../../../packages/zoen-ui/web/messages';
import type { FileDto } from './generated/FileDto';

export interface BrowserMediaHost {
  readonly closed: AbortSignal;
  readonly ownerId: string;
  readonly deviceId: string;
  saveProfile(
    request: { name: string; bio: string; photo: 'keep' | 'remove' | 'set'; mime?: string },
    bytes: Uint8Array,
  ): Promise<void>;
  profilePhoto(
    identityId: string,
    expectedSha: string,
  ): Promise<{ bytes: Uint8Array; mime: string; sha256: string }>;
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
export function activateBrowserMediaHost(
  messages: Messages,
  ownerId: string,
  deviceId: string,
): () => void {
  const controller = new AbortController();
  activeController?.abort();
  activeController = controller;
  const host: BrowserMediaHost = {
    closed: controller.signal,
    ownerId,
    deviceId,
    saveProfile: (request, bytes) => messages.saveProfile(request, bytes),
    profilePhoto: (identity, sha) => messages.profilePhoto(identity, sha),
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
