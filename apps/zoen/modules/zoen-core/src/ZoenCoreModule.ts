import { type NativeModule, requireOptionalNativeModule } from 'expo';
import type { CoreEvent } from '../../../src/core/types';

declare class ZoenCoreModule extends NativeModule<{ onChange: (event: CoreEvent) => void }> {
  open(mode: 'account' | 'demo'): Promise<string>;
  request(command: string): Promise<string>;
  createAccount(name: string, handle: string, relay: string): Promise<string>;
  sendFile(spaceId: string, uri: string, name: string, mime: string): Promise<string>;
  sendVoice(
    spaceId: string,
    uri: string,
    mime: string,
    durationMs: number,
    levels: number[],
    replyTo: string | null,
    thread: boolean,
  ): Promise<string>;
  fileInfo(itemId: string, version: number | null): Promise<string>;
  materializeFile(itemId: string, version: number | null): Promise<string>;
  releaseLocalFile(uri: string): Promise<void>;
  close(): Promise<void>;
}

export default requireOptionalNativeModule<ZoenCoreModule>('ZoenCore');
