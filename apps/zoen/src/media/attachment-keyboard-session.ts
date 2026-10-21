import type { MediaPickKind } from './media-actions.types';

export type KeyboardField = Readonly<{
  tag: number;
  context: string;
  label: string;
  placeholder: string;
}>;
export type KeyboardPresentation = Readonly<{
  id: string;
  kind: 'attachments' | 'emoji';
  field: KeyboardField;
  closing: boolean;
  returnToKeyboard: boolean;
}>;
export type AttachmentKeyboardState = Readonly<{
  enabled: boolean;
  field: KeyboardField | null;
  presentation: KeyboardPresentation | null;
  hasAudio: boolean;
}>;
type Work = {
  presentation: KeyboardPresentation;
  pickerId: string;
  field: KeyboardField;
  generation: number;
  pickerDone: boolean;
  pickerStarted: boolean;
  dispatchQueued: boolean;
  nativeClosed: boolean;
  next: 'attachments' | 'emoji' | 'document' | 'audio' | null;
};
type Choose = (kind: MediaPickKind) => Promise<void>;

/** Replaceable UI intent only. Photos and the durable draft keep their existing authorities. */
export class AttachmentKeyboardSession {
  private state: AttachmentKeyboardState = {
    enabled: false,
    field: null,
    presentation: null,
    hasAudio: false,
  };
  private listeners = new Set<() => void>();
  private generation = 0;
  private sequence = 0;
  private scope: string;
  private allowed: boolean;
  private foreground = true;
  private focused = true;
  private work: Work | null = null;
  private actions: { owner: object; choose: Choose; hasAudio: boolean; enabled: boolean } | null =
    null;
  private readonly token: () => string;
  constructor(scope: string, enabled: boolean, token: () => string) {
    this.token = token;
    this.scope = scope;
    this.allowed = enabled;
    this.state = { ...this.state, enabled };
  }
  getSnapshot = () => this.state;
  subscribe = (listener: () => void) => {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  };
  private publish(patch: Partial<AttachmentKeyboardState>) {
    this.state = { ...this.state, ...patch };
    for (const listener of this.listeners) listener();
  }
  private owned(work: Work) {
    return (
      this.work === work && work.generation === this.generation && this.state.field === work.field
    );
  }
  private live(work: Work) {
    return this.owned(work) && this.state.enabled;
  }
  configure(scope: string, enabled: boolean) {
    if (scope !== this.scope || enabled !== this.allowed) {
      this.retire();
      this.scope = scope;
      this.allowed = enabled;
    }
    this.publish({ enabled: this.allowed && this.foreground && this.focused });
  }
  setForeground(value: boolean, retire = !value) {
    if (retire) this.retire();
    this.foreground = value;
    this.publish({ enabled: this.allowed && this.foreground && this.focused });
  }
  setFocused(value: boolean) {
    if (!value) this.retire();
    this.focused = value;
    this.publish({ enabled: this.allowed && this.foreground && this.focused });
  }
  bind(field: KeyboardField) {
    this.retire();
    this.publish({ field });
    return () => {
      if (this.state.field !== field) return;
      this.retire();
      this.publish({ field: null });
    };
  }
  register(choose: Choose, hasAudio: boolean, enabled = true) {
    const owner = {};
    this.actions = { owner, choose, hasAudio, enabled };
    this.publish({ hasAudio });
    if (this.work) this.complete(this.work);
    return () => {
      if (this.actions?.owner !== owner) return;
      this.actions = null;
      this.publish({ hasAudio: false });
    };
  }
  private create(kind: KeyboardPresentation['kind']): Work | null {
    const field = this.state.field;
    if (!this.state.enabled || !field) return null;
    const presentation: KeyboardPresentation = {
      id: `${this.token()}:${++this.generation}:${++this.sequence}`,
      kind,
      field,
      closing: false,
      returnToKeyboard: false,
    };
    const work: Work = {
      presentation,
      pickerId: presentation.id,
      field,
      generation: this.generation,
      pickerDone: kind === 'emoji',
      pickerStarted: kind === 'emoji',
      dispatchQueued: false,
      nativeClosed: false,
      next: null,
    };
    this.work = work;
    this.publish({ presentation });
    return work;
  }
  openAttachments = () => {
    if (this.work) {
      if (this.work.presentation.kind === 'emoji')
        this.handoff(this.work.presentation.id, 'attachments');
      else this.closeKeyboardPanel();
      return;
    }
    const actions = this.actions;
    if (!actions?.enabled) return;
    const work = this.create('attachments');
    if (!work) return;
    let result: Promise<void>;
    try {
      result = actions.choose('photo');
    } catch {
      if (this.work === work) this.retire();
      return;
    }
    void Promise.resolve(result).then(
      () => {
        work.pickerDone = true;
        this.complete(work);
      },
      () => {
        work.pickerDone = true;
        if (this.work === work) this.retire();
      },
    );
  };
  toggleEmoji = () => {
    if (this.work) {
      if (this.work.presentation.kind === 'emoji') this.closeKeyboardPanel();
      else this.handoff(this.work.presentation.id, 'emoji');
      return;
    }
    this.create('emoji');
  };
  handoff(id: string, kind: Work['next']) {
    const work = this.work;
    if (
      !work ||
      !this.live(work) ||
      work.presentation.id !== id ||
      work.presentation.closing ||
      !kind
    )
      return;
    if (kind === 'audio' && !this.actions?.hasAudio) return;
    work.next = kind;
    work.presentation = {
      ...work.presentation,
      closing: true,
      returnToKeyboard: kind === 'emoji' || kind === 'attachments',
    };
    this.publish({ presentation: work.presentation });
  }
  closeKeyboardPanel = () => {
    const work = this.work;
    if (!work || !this.live(work) || work.presentation.closing) return;
    work.next = null;
    work.presentation = { ...work.presentation, closing: true, returnToKeyboard: true };
    this.publish({ presentation: work.presentation });
  };
  closed(id: string) {
    const work = this.work;
    if (!work || work.presentation.id !== id || !this.owned(work)) return false;
    work.nativeClosed = true;
    this.complete(work);
    return true;
  }
  admitPicker(id: string) {
    const work = this.work;
    if (!work || work.presentation.id !== id || !this.live(work) || work.presentation.closing)
      return false;
    work.pickerStarted = true;
    return true;
  }
  /** Resume the same Photos selection after Camera removed its native panel. */
  resumePicker(id: string): KeyboardPresentation | null {
    const work = this.work;
    if (
      !work ||
      !this.live(work) ||
      work.presentation.id !== id ||
      work.presentation.kind !== 'attachments' ||
      work.presentation.closing ||
      !work.nativeClosed ||
      !work.pickerStarted ||
      work.pickerDone
    )
      return null;
    // Each actual native presentation has a terminal token. Keep the picker
    // authority, but never reuse the removed panel's token or admit its events.
    work.presentation = {
      ...work.presentation,
      id: `${this.token()}:${work.generation}:${++this.sequence}`,
    };
    work.nativeClosed = false;
    this.publish({ presentation: work.presentation });
    return work.presentation;
  }
  /** Picker lifecycle ownership only; native actions must use isCurrent(). */
  ownsPicker(id: string) {
    return !!this.work && this.work.pickerId === id && this.owned(this.work);
  }
  private complete(work: Work) {
    if (!this.owned(work) || !work.pickerDone) return;
    // A rejected attachment admission never installed an inputView. There is
    // no native presentation to wait for or complete by a guessed delay.
    if (!work.pickerStarted) work.nativeClosed = true;
    if (!work.nativeClosed) return;
    const next = work.next;
    if (!this.state.enabled || !next || (next === 'audio' && !this.actions?.hasAudio)) {
      this.work = null;
      this.publish({ presentation: null });
      return;
    }
    this.publish({ presentation: null });
    if (next === 'emoji') {
      this.work = null;
      this.create('emoji');
      return;
    }
    if (!this.actions?.enabled || work.dispatchQueued) return;
    work.dispatchQueued = true;
    // Registration happens in a child layout effect. Finish the SAME React
    // commit before invoking its current callback, then revalidate owner and
    // readiness. Native removal and picker completion are already proven.
    queueMicrotask(() => {
      work.dispatchQueued = false;
      if (!this.owned(work) || !this.state.enabled) return;
      const actions = this.actions;
      if (!actions?.enabled || (next === 'audio' && !actions.hasAudio)) return;
      this.work = null;
      if (next === 'attachments') this.openAttachments();
      else void actions.choose(next).catch(() => {});
    });
  }
  owns(id: string) {
    return !!this.work && this.work.presentation.id === id && this.owned(this.work);
  }
  isCurrent(id: string) {
    return !!this.work && this.work.presentation.id === id && this.live(this.work);
  }
  retire = () => {
    ++this.generation;
    this.work = null;
    this.publish({ presentation: null });
  };
}
