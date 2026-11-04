import { useLayoutEffect, useMemo, useRef, useSyncExternalStore } from 'react';
import { AppState } from 'react-native';
import type { VideoPlayer } from 'expo-video';
import type { MediaPlaybackScope } from '../core/generated/MediaPlaybackScope';
import { useClient } from '../core/provider';
import { homeOwnerFor, homeSpacesFor } from '../home/access';
import { VideoCheckpoint, VideoCheckpointBinding } from './video-checkpoint';

/** The row/player keeps its existing lease. This adapter stores progress only. */
export function useVideoCheckpoint(
  player: VideoPlayer,
  scope?: MediaPlaybackScope,
  isCurrent?: () => boolean,
) {
  const { store } = useClient();
  const external = useRef(isCurrent);
  useLayoutEffect(() => {
    external.current = isCurrent;
  }, [isCurrent]);
  const encoded = JSON.stringify(scope ?? null);
  const binding = useMemo(() => new VideoCheckpointBinding(player, encoded), [player, encoded]);
  const state = useSyncExternalStore(binding.subscribe, binding.getSnapshot, binding.getSnapshot);
  useLayoutEffect(() => {
    const target = JSON.parse(encoded) as MediaPlaybackScope | null;
    if (!target) {
      binding.publish('ready');
      return;
    }
    const epoch = store.sessionEpoch;
    let live = true;
    const current = () => {
      const state = store.getSnapshot(),
        owner = homeOwnerFor(state);
      return (
        live &&
        !store.isSessionRetiring &&
        store.sessionEpoch === epoch &&
        owner?.mode === 'account' &&
        owner.identityId === target.expected_identity_id &&
        owner.deviceId === target.expected_device_id &&
        homeSpacesFor(state).has(target.space_id) &&
        external.current?.() !== false
      );
    };
    const session = new VideoCheckpoint(
      { scope: target, epoch },
      {
        player,
        current,
        read: (scope) => store.driver.request({ op: 'media_playback_read', scope }),
        save: (scope, expected_revision, position) =>
          store.driver.request({ op: 'media_playback_save', scope, expected_revision, position }),
        ready: () => {
          if (current()) binding.publish('ready');
        },
      },
    );
    binding.attach(session);
    const subscriptions = [
      player.addListener('timeUpdate', () => session.update()),
      player.addListener('sourceLoad', () => session.update()),
      player.addListener('playingChange', () => session.update()),
      player.addListener('playToEnd', () => {
        session.pause();
      }),
    ];
    const stop = store.subscribe(() => {
      if (!current()) {
        session.retire();
        if (live) binding.publish('unavailable');
      }
    });
    let verifying = false,
      again = false;
    const validate = async () => {
      if (verifying) {
        again = true;
        return;
      }
      verifying = true;
      try {
        do {
          again = false;
          const message = await store.driver.request({
            op: 'message',
            space_id: target.space_id,
            message_id: target.message_id,
          });
          if (!live) return;
          const card = 'Message' in message.kind ? message.kind.Message.card : null;
          if (
            !current() ||
            message.deleted ||
            card?.item_id !== target.item_id ||
            card.version !== target.version
          )
            throw new Error('The video message changed.');
        } while (again && live);
      } catch {
        if (live) {
          session.retire();
          binding.publish('unavailable');
        }
      } finally {
        verifying = false;
      }
    };
    const changed = store.subscribeCanonicalChanges((spaces) => {
      if (!spaces || spaces.includes(target.space_id)) void validate();
    });
    const app = AppState.addEventListener('change', (state) => {
      if (state !== 'active') session.pause();
    });
    const retire = async () => {
      session.retire();
      if (live) binding.publish('unavailable');
      await session.flush();
    };
    const cleanup = store.registerSessionCleanup(retire);
    const cleanupDevice = store.registerDeviceCleanup(retire);
    void session.open();
    return () => {
      session.retire();
      live = false;
      stop();
      changed();
      app.remove();
      for (const subscription of subscriptions) subscription.remove();
      void session.flush().finally(() => {
        cleanup();
        cleanupDevice();
      });
    };
  }, [player, store, encoded, binding]);
  return {
    ready: state === 'ready',
    unavailable: state === 'unavailable',
    checkpoint: binding.checkpoint,
  };
}
