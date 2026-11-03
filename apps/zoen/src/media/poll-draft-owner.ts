import { pollArguments } from './attachment-content.ts';
import { MediaDraftFailure, type MediaDraftChannel } from './media-draft-channel.ts';
import { contentValue } from './media-draft-contract.ts';
import { copyMediaReply, type MediaReplyTarget } from './media-reply.ts';
import type { MediaDraftPoll, MediaDraftSendResult } from './media-draft-types.ts';

/** The channel is the only question/options store. This lease only owns UI admission. */
export class PollDraftOwner {
  private active = false;
  private finished = false;
  private submission: Promise<MediaDraftSendResult> | null = null;
  readonly initial: MediaDraftPoll;
  readonly channel: MediaDraftChannel;
  private readonly current: () => boolean;
  private readonly nextId: () => string;

  constructor(
    channel: MediaDraftChannel,
    reply: MediaReplyTarget,
    current: () => boolean,
    nextId: () => string,
  ) {
    this.channel = channel;
    this.current = current;
    this.nextId = nextId;
    this.initial = contentValue({
      kind: 'poll',
      reply_to: reply.reply_to,
      ...copyMediaReply(reply),
      question: '',
      options: [
        { id: nextId(), text: '' },
        { id: nextId(), text: '' },
      ],
    }).content as MediaDraftPoll;
  }
  activate() {
    this.active = true;
  }
  retire() {
    this.active = false;
  }
  private admitted() {
    if (!this.active || this.finished || !this.current()) throw new MediaDraftFailure('closed');
    const state = this.channel.getSnapshot();
    if (state.phase !== 'ready') throw new MediaDraftFailure('notLoaded');
    if (state.content && state.content.kind !== 'poll') throw new MediaDraftFailure('conflict');
    return state;
  }
  private editable(): MediaDraftPoll {
    const state = this.admitted();
    if (this.submission || state.frozen || state.sending || state.reviewing)
      throw new MediaDraftFailure('busy');
    return state.content?.kind === 'poll' ? state.content : this.initial;
  }
  setQuestion(question: string) {
    return this.channel.save({ ...this.editable(), question });
  }
  setOption(id: string, text: string) {
    const content = this.editable();
    if (!content.options.some((option) => option.id === id))
      throw new MediaDraftFailure('conflict');
    return this.channel.save({
      ...content,
      options: content.options.map((option) => (option.id === id ? { ...option, text } : option)),
    });
  }
  addOption() {
    const content = this.editable();
    if (content.options.length >= 10) throw new MediaDraftFailure('invalid');
    return this.channel.save({
      ...content,
      options: [...content.options, { id: this.nextId(), text: '' }],
    });
  }
  removeOption(id: string) {
    const content = this.editable();
    if (content.options.length <= 2 || !content.options.some((option) => option.id === id))
      throw new MediaDraftFailure('invalid');
    return this.channel.save({
      ...content,
      options: content.options.filter((option) => option.id !== id),
    });
  }
  clearReply() {
    const content = this.editable();
    if (
      !content.shared_reply &&
      !content.reply_quote &&
      (!content.reply_to || this.channel.scope.thread_root_id)
    )
      return null;
    const previous = { reply_to: content.reply_to, ...copyMediaReply(content) };
    const saved = this.channel.save({
      ...content,
      reply_to: null,
      reply_quote: undefined,
      shared_reply: undefined,
      reply_intent: undefined,
    });
    return { previous, saved };
  }
  submit(): Promise<MediaDraftSendResult> {
    if (this.submission) return this.submission;
    const state = this.admitted();
    if (state.content?.kind !== 'poll') throw new Error('poll-invalid');
    pollArguments(
      state.content.question,
      state.content.options.map((option) => option.text),
    );
    // send() seals synchronously and drains prior saves. No flush-then-send gap.
    const task = this.channel.send().then((result) => {
      this.finished = true;
      return result;
    });
    this.submission = task;
    void task.then(
      () => {
        this.submission = null;
      },
      () => {
        this.submission = null;
      },
    );
    return task;
  }
  async discard() {
    this.admitted();
    if (this.submission || this.channel.getSnapshot().sending) throw new MediaDraftFailure('busy');
    await this.channel.clear();
    this.finished = true;
  }
}
