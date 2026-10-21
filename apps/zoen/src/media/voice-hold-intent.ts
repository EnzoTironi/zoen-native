import type { RecordingGestureEnd } from '../core/telegram-audio-presentation.ts';

type Ports = Readonly<{
  start: () => Promise<boolean>;
  review: () => Promise<void>;
  cancel: () => Promise<void>;
  locked: () => void;
  error: (reason: unknown) => void;
}>;
/** Ephemeral pointer intent only. The existing MediaComposer remains the sole native capture owner. */
export class VoiceHoldIntent {
  private live = true;
  private attempt: object | null = null;
  private terminal = false;
  private ports: Ports;
  constructor(ports: Ports) {
    this.ports = ports;
  }
  update(ports: Ports): void {
    this.ports = ports;
  }
  activate(): void {
    this.live = true;
  }
  begin(): void {
    if (!this.live || this.attempt) return;
    const attempt = {};
    this.attempt = attempt;
    this.terminal = false;
    // Admission is synchronous: a release during permission/prepare sees the owner's start task.
    try {
      void this.ports.start().then(
        (started) => {
          if (!started && this.attempt === attempt) this.attempt = null;
        },
        (error) => {
          if (this.live && this.attempt === attempt) this.ports.error(error);
          if (this.attempt === attempt) this.attempt = null;
        },
      );
    } catch (error) {
      this.attempt = null;
      if (this.live) this.ports.error(error);
    }
  }
  end(decision: RecordingGestureEnd): void {
    if (!this.live || !this.attempt || this.terminal) return;
    this.terminal = true;
    if (decision === 'lock') {
      this.ports.locked();
      return;
    }
    const attempt = this.attempt;
    try {
      // Stop/cancel own their existing single-flight drain, including an unfinished start.
      void (decision === 'cancel' ? this.ports.cancel() : this.ports.review())
        .catch((error: unknown) => {
          if (this.live && this.attempt === attempt) this.ports.error(error);
        })
        .finally(() => {
          if (this.attempt === attempt) this.attempt = null;
        });
    } catch (error) {
      this.attempt = null;
      if (this.live) this.ports.error(error);
    }
  }
  completed(): void {
    this.attempt = null;
    this.terminal = false;
  }
  retire(): void {
    this.live = false;
    this.attempt = null;
    // Native cleanup belongs to the capture owner, never a late gesture continuation.
  }
}
