type Owners = {
  begin(ticket: string): void;
  owns(ticket: string): boolean;
  end(ticket: string): void;
  pause(): void;
  stopCapture(): Promise<void>;
  drain(): Promise<void>;
  deactivateExpo(): Promise<void>;
  reply(ticket: string, success: boolean): Promise<void>;
};

/** Serialize native ownership changes across asynchronous Expo media cleanup. */
export function createCallAudioHandoff(owners: Owners) {
  let tail = Promise.resolve();
  return (json: string): Promise<void> => {
    if (json.length > 512) return tail;
    let value: unknown;
    try {
      value = JSON.parse(json);
    } catch {
      return tail;
    }
    if (!value || typeof value !== 'object' || !('ticket' in value) || !('action' in value))
      return tail;
    const { ticket, action } = value;
    if (
      typeof ticket !== 'string' ||
      !/^[a-f0-9-]{36}$/i.test(ticket) ||
      (action !== 'acquire' && action !== 'adopt' && action !== 'release')
    )
      return tail;
    tail = tail
      .then(async () => {
        if (action === 'release') {
          if (owners.owns(ticket)) {
            owners.pause();
            owners.end(ticket);
          }
          return;
        }
        try {
          owners.begin(ticket);
          owners.pause();
          await owners.stopCapture();
          await owners.drain();
          // Cold adoption already has CallKit's live audio session. Fence newly
          // mounted JS owners without deactivating that ongoing conversation.
          if (action === 'acquire') await owners.deactivateExpo();
          await owners.reply(ticket, true);
        } catch {
          await owners.reply(ticket, false);
        }
      })
      .catch(() => {});
    return tail;
  };
}
