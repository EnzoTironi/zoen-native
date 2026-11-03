import { projectVoiceDraft } from './voice-edit.ts';
import type { VoiceEditorState } from './voice-editor-state.ts';
import type { MediaDraftChannel } from './media-draft-channel.ts';
import type { MediaDraftVoice, MediaDraftSendResult } from './media-draft-types.ts';
import { VoiceReviewOwner } from './voice-review-owner.ts';
import {
  VoiceSessionFailure,
  type VoiceNativePort,
  type VoicePreparedSource,
  type VoiceRenderResult,
  type VoiceSendable,
} from './voice-session.ts';
import type { SensitiveCacheWork } from './cache-work.ts';
import { captureMediaReply, type MediaReplyOptions, type MediaReplyTarget } from './media-reply.ts';

type Options = MediaReplyOptions & {
  channel: MediaDraftChannel;
  port: VoiceNativePort;
  uri: string;
  replyTo: string | null;
  work: Pick<SensitiveCacheWork, 'run' | 'track'>;
  registerCleanup: (cleanup: () => Promise<void>) => () => void;
  discard: (uri: string) => void | Promise<void>;
  onError: (error: unknown) => void;
  id: () => string;
  restored?: MediaDraftVoice;
  frozenPreview?: VoiceRenderResult;
  releaseMaterialized?: () => Promise<void>;
};

/** A route owns plaintext leases; its session channel owns recoverable encrypted intent. */
export class VoiceDraftOwner {
  readonly owner: VoiceReviewOwner;
  readonly channel: MediaDraftChannel;
  private content: MediaDraftVoice;
  private editorJson = '';
  private unsubscribe: (() => void) | null;
  private render: { preview: VoiceRenderResult; id: string } | null = null;
  private readonly onError: Options['onError'];
  private save: MediaDraftChannel['save'];
  private preparing: Promise<void> | null = null;
  private readonly id: () => string;

  constructor(options: Options) {
    this.channel = options.channel;
    this.save = options.channel.save.bind(options.channel);
    this.onError = options.onError;
    this.id = options.id;
    this.content = options.restored ?? {
      kind: 'voice',
      reply_to: options.replyTo,
      ...captureMediaReply(options),
      source_asset_id: `source_${this.id()}`,
      editor: null,
      render: null,
    };
    this.owner = new VoiceReviewOwner({
      ...options,
      editor: options.restored?.editor,
      logicalSourceId: this.content.source_asset_id,
      attempted: options.channel.getSnapshot().frozen,
      frozenPreview: options.frozenPreview,
      beforeReady: (source, editor) => this.prepared(source, editor, !!options.restored),
    });
    this.unsubscribe = this.owner.session.subscribe(() => this.changed());
  }
  get replyContext() {
    return {
      reply_to: this.content.reply_to,
      ...captureMediaReply({
        replyQuote: this.content.reply_quote,
        sharedReply: this.content.shared_reply,
        replyIntent: this.content.reply_intent,
      }),
    };
  }

  clearReply(): MediaReplyTarget | null {
    const state = this.channel.getSnapshot();
    const voice = this.owner.session.getSnapshot();
    if (
      state.frozen ||
      state.sending ||
      state.phase !== 'ready' ||
      voice.phase !== 'ready' ||
      voice.attempted ||
      (!this.content.reply_to && !this.content.shared_reply) ||
      (this.channel.scope.thread_root_id && !this.content.reply_quote && !this.content.shared_reply)
    )
      return null;
    const previous = this.replyContext;
    this.content = {
      ...this.content,
      reply_to: null,
      reply_quote: undefined,
      shared_reply: undefined,
      reply_intent: undefined,
    };
    void this.save(this.content).catch(this.onError);
    return previous;
  }

  private async prepared(source: VoicePreparedSource, editor: VoiceEditorState, restored: boolean) {
    const importedFinalRender = restored && this.content.editor === null;
    const importedAsset = importedFinalRender
      ? this.channel
          .getSnapshot()
          .value?.assets.find((asset) => asset.id === this.content.source_asset_id)
      : undefined;
    if (importedFinalRender && !importedAsset) throw new VoiceSessionFailure('prepare');
    this.editorJson = JSON.stringify(editor);
    this.content = { ...this.content, editor };
    if (restored && !importedFinalRender) return;
    const extension = source.uri.split('.').at(-1)?.toLowerCase();
    const mime =
      extension === 'wav'
        ? 'audio/wav'
        : extension === 'mp3'
          ? 'audio/mpeg'
          : extension === 'ogg'
            ? 'audio/ogg'
            : extension === 'webm'
              ? 'audio/webm'
              : 'audio/mp4';
    await this.save(this.content, [
      {
        id: this.content.source_asset_id,
        uri: source.uri,
        name: importedAsset?.name ?? `original.${extension || 'm4a'}`,
        mime: importedAsset?.mime ?? mime,
        voice_source_id: source.id,
        voice_role: 'source',
      },
    ]);
  }
  private changed() {
    const state = this.owner.session.getSnapshot();
    if (state.phase !== 'ready' || state.attempted || this.channel.getSnapshot().frozen) return;
    const editor = this.owner.session.editorState();
    if (!editor) return;
    const json = JSON.stringify(editor);
    if (json === this.editorJson) return;
    this.editorJson = json;
    this.content = { ...this.content, editor, render: null };
    void this.channel.save(this.content).catch(this.onError);
  }

  prepare(): Promise<void> {
    if (this.preparing) return this.preparing;
    const task = this.channel.stage(async (save) => {
      this.save = save;
      try {
        await this.owner.prepare();
        const state = this.owner.session.getSnapshot();
        if (state.phase !== 'ready') throw state.error ?? new VoiceSessionFailure('prepare');
      } finally {
        this.save = this.channel.save.bind(this.channel);
      }
    });
    this.preparing = task;
    const settled = () => {
      if (this.preparing === task) this.preparing = null;
    };
    void task.then(settled, settled);
    return task;
  }

  async send(captured: VoiceSendable): Promise<MediaDraftSendResult> {
    let accepted: MediaDraftSendResult | undefined;
    await this.owner.send(captured, async () => {
      if (!this.channel.getSnapshot().frozen) {
        await this.channel.flush();
        const source = this.owner.session.getSnapshot().source;
        const current = this.owner.session.getSendable();
        const editor = this.owner.session.editorState();
        if (
          !source ||
          !editor ||
          current?.preview !== captured.preview ||
          current.transcript !== captured.transcript
        )
          throw new VoiceSessionFailure('not_ready');
        if (!this.render || this.render.preview !== captured.preview)
          this.render = { preview: captured.preview, id: `render_${this.id()}` };
        const content: MediaDraftVoice = {
          ...this.content,
          editor,
          render: {
            asset_id: this.render.id,
            mime: captured.preview.mime,
            duration_ms: captured.preview.durationMs,
            levels: [...captured.preview.levels],
            transcript: captured.transcript,
          },
        };
        await this.channel.save(content, [
          {
            id: this.render.id,
            uri: captured.preview.uri,
            name: 'voice.m4a',
            mime: captured.preview.mime,
            voice_source_id: source.id,
            voice_role: 'render',
          },
        ]);
        if (this.owner.session.getSendable()?.preview !== captured.preview)
          throw new VoiceSessionFailure('not_ready');
        this.content = content;
      }
      this.owner.session.freeze();
      try {
        accepted = await this.channel.send();
      } catch (error) {
        if (!this.channel.getSnapshot().frozen) this.owner.session.freeze(false);
        throw error;
      }
    });
    if (!accepted) throw new VoiceSessionFailure('not_ready');
    return accepted;
  }

  retire() {
    this.unsubscribe?.();
    this.unsubscribe = null;
    this.channel.deferRelease(() => this.owner.close());
  }
  async discard() {
    await this.channel.clear();
    this.unsubscribe?.();
    this.unsubscribe = null;
    await this.owner.close();
  }
}

export function frozenVoicePreview(content: MediaDraftVoice, uri: string): VoiceRenderResult {
  if (!content.render) throw new VoiceSessionFailure('invalid_render');
  const editor = content.editor as VoiceEditorState;
  return {
    uri,
    mime: content.render.mime,
    durationMs: content.render.duration_ms,
    frameCount: projectVoiceDraft(editor.draft).frameCount,
    levels: [...content.render.levels],
  };
}
