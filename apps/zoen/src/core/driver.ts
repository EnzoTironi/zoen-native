import { createNativePageInputPort } from '../documents/native-page-input';
import NativeCore from '../../modules/zoen-core/src/ZoenCoreModule';
import type { Command, CommandResult, CoreDriver, Snapshot } from './types';
import { createNativeProfilePhotoDriver } from '../profile/native-driver';
import type { MediaDriver } from '../media/types';
import { nativeContactReader } from '../media/native-contact';
import { nativeLocationReader, nativeLocationSearch } from '../media/native-location';
import { Platform } from 'react-native';
import { locale as appLocale } from '../i18n/index.ts';
import { randomUUID } from 'expo-crypto';

function core() {
  if (!NativeCore)
    throw new Error(
      'Este app precisa do build de desenvolvimento do Zoen. Compile com npm run ios ou npm run android.',
    );
  return NativeCore;
}

export function createDriver(): CoreDriver {
  const location =
    Platform.OS === 'ios' &&
    NativeCore?.staticLocationVersion === 1 &&
    typeof NativeCore.searchLocations === 'function' &&
    typeof NativeCore.cancelLocationSearch === 'function'
      ? nativeLocationSearch({
          searchLocations: (request) => core().searchLocations!(request),
          cancelLocationSearch: (binding) => core().cancelLocationSearch!(binding),
        })
      : undefined;
  const media: MediaDriver = {
    searchLocations: location?.searchLocations,
    cancelLocationSearch: location?.cancelLocationSearch,
    readLocation:
      Platform.OS === 'ios' &&
      NativeCore?.staticLocationVersion === 1 &&
      typeof NativeCore.readLocationFile === 'function'
        ? nativeLocationReader({ readLocationFile: (request) => core().readLocationFile!(request) })
        : undefined,
    recentAttachments:
      Platform.OS === 'ios' &&
      typeof NativeCore?.materializeRecentAttachment === 'function' &&
      typeof NativeCore?.previewRecentAttachment === 'function' &&
      typeof NativeCore?.cancelMediaDraftPreview === 'function'
        ? {
            async list(scope, kind, query, cursor) {
              return JSON.parse(
                await core().request(
                  JSON.stringify({
                    op: 'recent_attachments',
                    scope,
                    kind,
                    query,
                    cursor,
                    limit: 50,
                  }),
                ),
              );
            },
            async materialize(scope, kind, reference) {
              return JSON.parse(
                await core().materializeRecentAttachment!(
                  JSON.stringify({ scope, kind, reference }),
                ),
              );
            },
            async preview(scope, kind, reference, signal) {
              if (signal.aborted) return;
              const token = randomUUID();
              const cancel = () => {
                void core().cancelMediaDraftPreview!(token).catch(() => {});
              };
              signal.addEventListener('abort', cancel, { once: true });
              try {
                await core().previewRecentAttachment!(
                  JSON.stringify({ scope, kind, reference }),
                  token,
                );
              } finally {
                signal.removeEventListener('abort', cancel);
              }
            },
            release: (uri) => core().releaseLocalFile(uri),
          }
        : undefined,
    drafts:
      Platform.OS === 'ios' && typeof NativeCore?.loadMediaDraft === 'function'
        ? {
            async load(scope) {
              return JSON.parse(await core().loadMediaDraft(JSON.stringify({ scope })));
            },
            recoverFailed:
              NativeCore?.failedMediaRecoveryVersion === 1
                ? async (scope, messageId, expectedRevision, expectedMessageRevision) => {
                    const value = JSON.parse(
                      await core().request(
                        JSON.stringify({
                          op: 'recover_failed_media_draft',
                          scope,
                          message_id: messageId,
                          expected_revision: expectedRevision,
                          expected_message_revision: expectedMessageRevision,
                        }),
                      ),
                    );
                    // The general JSON API carries u64 as decimal strings. The media
                    // journal validator admits only exact, bounded draft asset sizes.
                    return {
                      ...value,
                      assets: value.assets.map((asset: { size: string }) => ({
                        ...asset,
                        size: Number(asset.size),
                      })),
                    };
                  }
                : undefined,
            async save(scope, expectedRevision, contentJson, assets) {
              return JSON.parse(
                await core().saveMediaDraft(
                  JSON.stringify({
                    scope,
                    expected_revision: expectedRevision,
                    content_json: contentJson,
                    assets,
                  }),
                ),
              );
            },
            async materialize(scope, expectedRevision, assetId) {
              return JSON.parse(
                await core().materializeMediaDraftAsset(
                  JSON.stringify({
                    scope,
                    expected_revision: expectedRevision,
                    asset_id: assetId,
                  }),
                ),
              );
            },
            preview:
              typeof NativeCore?.previewMediaDraftAsset === 'function' &&
              typeof NativeCore?.cancelMediaDraftPreview === 'function'
                ? (scope, expectedRevision, assetId, presentationId) =>
                    core().previewMediaDraftAsset(
                      JSON.stringify({
                        scope,
                        expected_revision: expectedRevision,
                        asset_id: assetId,
                      }),
                      presentationId,
                    )
                : undefined,
            cancelPreview:
              typeof NativeCore?.cancelMediaDraftPreview === 'function'
                ? (presentationId) => core().cancelMediaDraftPreview(presentationId)
                : undefined,
            edit:
              typeof NativeCore?.editMediaDraftAsset === 'function' &&
              typeof NativeCore?.cancelMediaDraftPreview === 'function'
                ? async (scope, expectedRevision, assetId, presentationId) =>
                    JSON.parse(
                      await core().editMediaDraftAsset(
                        JSON.stringify({
                          scope,
                          expected_revision: expectedRevision,
                          asset_id: assetId,
                        }),
                        presentationId,
                      ),
                    )
                : undefined,
            async send(scope, expectedRevision) {
              return JSON.parse(
                await core().sendMediaDraft(
                  JSON.stringify({ scope, expected_revision: expectedRevision }),
                ),
              );
            },
            release: (uri) => core().releaseLocalFile(uri),
          }
        : undefined,
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
        await core().sendFile(
          JSON.stringify({
            spaceId: selection.spaceId,
            uri: selection.uri,
            name: selection.name,
            mime: selection.mime,
            caption: selection.caption ?? '',
            replyTo: selection.replyTo,
            thread: selection.thread,
            mutationId: selection.mutationId,
          }),
        ),
      );
    },
    async sendVoice(recording) {
      return JSON.parse(
        await core().sendVoice(
          JSON.stringify({
            spaceId: recording.spaceId,
            uri: recording.uri,
            mime: recording.mime,
            durationMs: recording.durationMs,
            levels: recording.levels,
            replyTo: recording.replyTo,
            thread: recording.thread,
            transcript: recording.transcript ?? '',
            sourceId: recording.voiceSourceId ?? '',
          }),
        ),
      );
    },
    async fileInfo(itemId, version) {
      return JSON.parse(await core().fileInfo(itemId, version ?? null));
    },
    async materializeFile(itemId, version) {
      return JSON.parse(await core().materializeFile(itemId, version ?? null));
    },
    readContact:
      Platform.OS === 'ios' && typeof NativeCore?.readContactFile === 'function'
        ? nativeContactReader({ readContactFile: (request) => core().readContactFile!(request) })
        : undefined,
    previewFile:
      Platform.OS === 'ios' && typeof NativeCore?.previewLocalFile === 'function'
        ? async (file) => {
            await core().previewLocalFile(file.uri);
          }
        : undefined,
    previewConversationMedia:
      Platform.OS === 'ios' &&
      typeof NativeCore?.previewConversationMedia === 'function' &&
      typeof NativeCore?.cancelConversationMedia === 'function'
        ? async (request, signal) => {
            if (signal.aborted) return;
            const token = randomUUID();
            const cancel = () => {
              void core().cancelConversationMedia!(token).catch(() => {});
            };
            signal.addEventListener('abort', cancel, { once: true });
            try {
              await core().previewConversationMedia!(JSON.stringify(request), token);
            } finally {
              signal.removeEventListener('abort', cancel);
            }
          }
        : undefined,
    async releaseLocalFile(uri) {
      await core().releaseLocalFile(uri);
    },
  };
  return {
    durableMessageDrafts: true,
    localIntentId: randomUUID,
    pageInput: Platform.OS === 'ios' ? createNativePageInputPort(NativeCore) : undefined,
    assistant:
      Platform.OS === 'ios' && typeof NativeCore?.openAssistant === 'function'
        ? {
            async open(spaceId, locale) {
              return JSON.parse(await core().openAssistant(spaceId, locale));
            },
            async ask(id, question) {
              return JSON.parse(await core().askAssistant(id, question));
            },
            async close(id) {
              await core().closeAssistant(id);
            },
          }
        : undefined,
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
    async close(options) {
      if (options?.force && typeof NativeCore?.closeForce === 'function')
        await NativeCore.closeForce();
      else await NativeCore?.close();
    },
  };
}
