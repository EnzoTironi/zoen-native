import NativeCore from '../../modules/zoen-core/src/ZoenCoreModule';
import type { Command, CommandResult, CoreDriver, Snapshot } from './types';
import { createNativeProfilePhotoDriver } from '../profile/native-driver';
import type { MediaDriver } from '../media/types';
import { Platform } from 'react-native';
import { locale as appLocale } from '../i18n/index.ts';

function core() {
  if (!NativeCore)
    throw new Error(
      'Este app precisa do build de desenvolvimento do Zoen. Compile com npm run ios ou npm run android.',
    );
  return NativeCore;
}

export function createDriver(): CoreDriver {
  const media: MediaDriver = {
    voiceEditor:
      Platform.OS === 'ios' && typeof NativeCore?.prepareVoice === 'function'
        ? {
            async prepare(uri) {
              return JSON.parse(await core().prepareVoice(uri));
            },
            async render(sourceId, ranges) {
              return JSON.parse(await core().renderVoice(sourceId, JSON.stringify(ranges)));
            },
            async transcribe(sourceId, locale) {
              return JSON.parse(await core().transcribeVoice(sourceId, locale));
            },
            async releaseRender(sourceId, uri) {
              await core().releaseVoiceRender(sourceId, uri);
            },
            async cancel(sourceId) {
              await core().cancelVoice(sourceId);
            },
            async release(sourceId) {
              await core().releaseVoice(sourceId);
            },
          }
        : undefined,
    async sendFile(selection) {
      return JSON.parse(
        await core().sendFile(selection.spaceId, selection.uri, selection.name, selection.mime),
      );
    },
    async sendVoice(recording) {
      return JSON.parse(
        await core().sendVoice(
          recording.spaceId,
          recording.uri,
          recording.mime,
          recording.durationMs,
          recording.levels,
          recording.replyTo,
          recording.thread,
          recording.transcript ?? '',
        ),
      );
    },
    async fileInfo(itemId, version) {
      return JSON.parse(await core().fileInfo(itemId, version ?? null));
    },
    async materializeFile(itemId, version) {
      return JSON.parse(await core().materializeFile(itemId, version ?? null));
    },
    async releaseLocalFile(uri) {
      await core().releaseLocalFile(uri);
    },
  };
  return {
    media: Platform.OS === 'ios' ? media : undefined,
    photos: Platform.OS === 'ios' ? createNativeProfilePhotoDriver() : undefined,
    async open(mode) {
      return JSON.parse(await core().open(mode, appLocale)) as Snapshot;
    },
    async request<C extends Command>(command: C) {
      return JSON.parse(await core().request(JSON.stringify(command))) as CommandResult<C>;
    },
    async createAccount(name, handle, relay) {
      return JSON.parse(await core().createAccount(name, handle, relay)) as Snapshot;
    },
    subscribe(listener) {
      const subscription = core().addListener('onChange', listener);
      return () => subscription.remove();
    },
    async eraseDevice(expected) {
      await core().eraseDevice(expected.identity_id, expected.device_id);
    },
    async close() {
      await NativeCore?.close();
    },
  };
}
