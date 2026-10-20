import type {
  Messages,
  FileDoc,
  VoiceMeta,
  AttachmentIntent,
} from '../../../../packages/zoen-ui/web/messages';
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
  newAttachmentMutationId(): Promise<string>;
  releaseAttachmentMutation(mutationId: string, accepted: boolean): Promise<void>;
  sendFile(
    space: string,
    file: {
      name: string;
      mime: string;
      bytes: Uint8Array;
      thumb?: Uint8Array | null;
      voice?: VoiceMeta;
      attachment?: AttachmentIntent;
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
    newAttachmentMutationId: () => messages.newAttachmentMutationId(),
    releaseAttachmentMutation: (mutationId, accepted) =>
      messages.releaseAttachmentMutation(mutationId, accepted),
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
