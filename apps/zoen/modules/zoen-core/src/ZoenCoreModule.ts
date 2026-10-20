import { type NativeModule, requireOptionalNativeModule } from 'expo';
import type { CoreEvent } from '../../../src/core/types';

export type ComposerMeasurement = {
  tag: number;
  nonce: number;
  width: number;
  height: number;
  fontSize: number;
};

declare class ZoenCoreModule extends NativeModule<{
  onChange: (event: CoreEvent) => void;
  onPageInputStatus: (event: { json: string }) => void;
}> {
  measureComposer?(
    tag: number,
    expectedText: string,
    expectedLabel: string,
    expectedPlaceholder: string,
    width: number,
    expectedFontSize: number,
    nonce: number,
  ): Promise<ComposerMeasurement | null>;
  open(mode: 'account' | 'demo', locale: 'en' | 'pt-BR'): Promise<string>;
  request(command: string): Promise<string>;
  pageInputSession(): Promise<string>;
  loadPageInput(session: string, itemId: string): Promise<string>;
  reservePageInput(session: string, itemId: string): Promise<string>;
  openPageInput(request: string): Promise<string>;
  pageInputStatus(session: string, id: string): Promise<string>;
  sealPageInput(session: string, id: string): Promise<string>;
  applyPageInput(session: string, id: string): Promise<string>;
  discardPageInput(session: string, id: string): Promise<string>;
  closePageInput(session: string, id: string): Promise<void>;
  openAssistant(spaceId: string, locale: 'en' | 'pt-BR'): Promise<string>;
  askAssistant(id: string, question: string): Promise<string>;
  closeAssistant(id: string): Promise<void>;
  createAccount(name: string, handle: string, relay: string): Promise<string>;
  sendFile(request: string): Promise<string>;
  sendVoice(request: string): Promise<string>;
  loadMediaDraft(request: string): Promise<string>;
  saveMediaDraft(request: string): Promise<string>;
  materializeMediaDraftAsset(request: string): Promise<string>;
  previewMediaDraftAsset(request: string, presentationId: string): Promise<void>;
  cancelMediaDraftPreview(presentationId: string): Promise<void>;
  sendMediaDraft(request: string): Promise<string>;
  prepareVoice(uri: string): Promise<string>;
  renderVoice(sourceId: string, ranges: string): Promise<string>;
  transcribeVoice(sourceId: string, locale: string): Promise<string>;
  releaseVoiceRender(sourceId: string, uri: string): Promise<void>;
  cancelVoice(sourceId: string): Promise<void>;
  releaseVoice(sourceId: string): Promise<void>;
  fileInfo(itemId: string, version: number | null): Promise<string>;
  materializeFile(itemId: string, version: number | null): Promise<string>;
  previewLocalFile(uri: string): Promise<void>;
  releaseLocalFile(uri: string): Promise<void>;
  saveProfile(request: string): Promise<void>;
  materializeProfilePhoto(
    ownerId: string,
    deviceId: string,
    identityId: string,
    expectedSha256: string,
  ): Promise<string>;
  releaseProfilePhoto(uri: string): Promise<void>;
  close(): Promise<void>;
  closeForce?(): Promise<void>;
  eraseDevice(identityId: string, deviceId: string): Promise<void>;
}

export default requireOptionalNativeModule<ZoenCoreModule>('ZoenCore');
