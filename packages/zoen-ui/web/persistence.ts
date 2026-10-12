export async function persistBeforeOutgoing<T>({ snapshot, save, frames, emit }: {
  snapshot: T; save: (snapshot: T) => Promise<void>; frames: readonly Uint8Array[]; emit: (frame: Uint8Array) => void;
}): Promise<void> {
  await save(snapshot);
  for (const frame of frames) emit(frame);
}
