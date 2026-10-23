export type CallPresentation = {
  identity_id: string;
  device_id: string;
  space_id: string;
  call_id: string;
  state: 'calling' | 'incoming' | 'connecting' | 'connected' | 'reconnecting';
  minimized: boolean;
  connected_at_ms: number | null;
};

/** Native presentation data never grants call/RTC authority. */
export function readCallPresentation(
  json: string,
  owner: { identity: string | undefined; device: string | undefined },
): CallPresentation | null {
  if (!owner.identity || !owner.device || json.length > 2048) return null;
  let value: unknown;
  try {
    value = JSON.parse(json);
  } catch {
    return null;
  }
  if (!value || typeof value !== 'object') return null;
  const record = value as Record<string, unknown>;
  if (
    record.identity_id !== owner.identity ||
    record.device_id !== owner.device ||
    typeof record.space_id !== 'string' ||
    record.space_id.length > 128 ||
    !record.space_id ||
    typeof record.call_id !== 'string' ||
    !/^call_[a-f0-9]{32}$/.test(record.call_id) ||
    typeof record.state !== 'string' ||
    !['calling', 'incoming', 'connecting', 'connected', 'reconnecting'].includes(record.state) ||
    typeof record.minimized !== 'boolean' ||
    (record.connected_at_ms !== null &&
      (typeof record.connected_at_ms !== 'number' ||
        !Number.isSafeInteger(record.connected_at_ms) ||
        record.connected_at_ms < 0))
  )
    return null;
  return record as CallPresentation;
}

export function callElapsed(connectedAtMs: number | null, now: number): string | null {
  if (connectedAtMs === null) return null;
  const elapsed = Math.max(0, Math.floor((now - connectedAtMs) / 1000));
  const seconds = String(elapsed % 60).padStart(2, '0');
  const minutes = Math.floor(elapsed / 60);
  return `${minutes}:${seconds}`;
}

/** Subscribe before querying so a slow initial read cannot revive an ended call. */
export function watchCallPresentation(port: {
  listen(changed: (json: string) => void): () => void;
  current(): Promise<string>;
  owner(): { identity: string | undefined; device: string | undefined };
  changed(value: CallPresentation | null): void;
}): () => void {
  let live = true;
  let revision = 0;
  const apply = (json: string) => {
    if (live) port.changed(readCallPresentation(json, port.owner()));
  };
  const remove = port.listen((json) => {
    revision += 1;
    apply(json);
  });
  const atQuery = revision;
  void port
    .current()
    .then((json) => {
      if (revision === atQuery) apply(json);
    })
    .catch(() => {});
  return () => {
    live = false;
    remove();
  };
}
