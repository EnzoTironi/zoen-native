import type { VideoPlayer, VideoThumbnail } from 'expo-video';

export type DraftThumbnailPlayer = Pick<
  VideoPlayer,
  'status' | 'addListener' | 'generateThumbnailsAsync' | 'release'
>;

const SOURCE_TIMEOUT_MS = 15_000;

/** The SDK constructor starts loading asynchronously; an empty thumbnail result
 * before readyToPlay is not a successful extraction. Subscribe before sampling
 * status so an already-loaded local source cannot miss its readiness event. */
function sourceReady(player: DraftThumbnailPlayer, signal: AbortSignal): Promise<boolean> {
  return new Promise((resolve, reject) => {
    let settled = false;
    let subscription: { remove(): void } | undefined;
    let timeout: ReturnType<typeof setTimeout> | undefined;
    const abort = () => finish(false);
    function finish(ready: boolean, error?: Error) {
      if (settled) return;
      settled = true;
      subscription?.remove();
      signal.removeEventListener('abort', abort);
      if (timeout !== undefined) clearTimeout(timeout);
      if (error) reject(error);
      else resolve(ready);
    }
    function status(value: VideoPlayer['status']) {
      if (value === 'readyToPlay') finish(true);
      else if (value === 'error') finish(false, new Error('Video thumbnail source failed'));
    }
    try {
      subscription = player.addListener('statusChange', (event) => status(event.status));
      // Keep cleanup correct even for a port that reports readiness while the
      // subscription is being installed.
      if (settled) {
        subscription.remove();
        return;
      }
      signal.addEventListener('abort', abort, { once: true });
      if (signal.aborted) finish(false);
      else status(player.status);
      if (!settled)
        timeout = setTimeout(
          () => finish(false, new Error('Video thumbnail source timed out')),
          SOURCE_TIMEOUT_MS,
        );
    } catch {
      finish(false, new Error('Video thumbnail source failed'));
    }
  });
}

/** One native decoder at a time. A cancelled queued job never creates a player;
 * an admitted extraction drains before releasing it and allowing the next job. */
export function createDraftVideoThumbnails(createPlayer: (uri: string) => DraftThumbnailPlayer) {
  let queue: Promise<unknown> = Promise.resolve();
  return (uri: string, signal: AbortSignal): Promise<VideoThumbnail | null> => {
    const work = queue.then(async () => {
      if (signal.aborted) return null;
      const player = createPlayer(uri);
      try {
        if (!(await sourceReady(player, signal)) || signal.aborted) return null;
        const images = await player.generateThumbnailsAsync([0], {
          maxWidth: 192,
          maxHeight: 192,
        });
        if (signal.aborted) {
          for (const image of images) image.release();
          return null;
        }
        for (const image of images.slice(1)) image.release();
        return images[0] ?? null;
      } finally {
        player.release();
      }
    });
    queue = work.catch(() => {});
    return work;
  };
}
