import type { MessageInfo } from './generated/MessageInfo';
import type { MessageInfoScope } from './generated/MessageInfoScope';

export type MessageInfoPort = {
  current(): boolean;
  read(scope: MessageInfoScope): Promise<MessageInfo>;
};
export class MessageInfoSession {
  private focused = false;
  private version = 0;
  private pending = false;
  private again = false;
  private readonly scope: MessageInfoScope;
  private readonly port: MessageInfoPort;
  private readonly publish: (value: MessageInfo | null, loading: boolean, error: boolean) => void;
  constructor(
    scope: MessageInfoScope,
    port: MessageInfoPort,
    publish: (value: MessageInfo | null, loading: boolean, error: boolean) => void,
  ) {
    this.scope = Object.freeze({ ...scope });
    this.port = port;
    this.publish = publish;
  }
  private current() {
    return this.focused && this.port.current();
  }
  focus() {
    this.focused = true;
    this.refresh();
  }
  blur() {
    this.focused = false;
    ++this.version;
    this.again = false;
    this.publish(null, false, false);
  }
  checkAuthority() {
    if (this.focused && !this.port.current()) this.refresh();
  }
  refresh() {
    ++this.version;
    if (!this.current()) {
      this.again = false;
      this.publish(null, false, false);
      return;
    }
    if (this.pending) {
      this.again = true;
      return;
    }
    void this.load();
  }
  private async load() {
    if (!this.current()) return;
    this.pending = true;
    this.again = false;
    const version = this.version;
    this.publish(null, true, false);
    try {
      const value = await this.port.read(this.scope);
      if (!this.current() || version !== this.version) return;
      if (
        Object.keys(this.scope).some(
          (key) =>
            value.scope[key as keyof MessageInfoScope] !==
            this.scope[key as keyof MessageInfoScope],
        ) ||
        new Set(value.participants.map((p) => p.person.id)).size !== value.participants.length
      )
        throw new Error('Message metadata scope changed');
      this.publish(value, false, false);
    } catch {
      if (this.current() && version === this.version) this.publish(null, false, true);
    } finally {
      this.pending = false;
      if (this.current() && this.again) void this.load();
    }
  }
}
