import { useCallback, useRef, useState, useSyncExternalStore } from 'react';
import { AppState, Text, View } from 'react-native';
import { Image } from 'expo-image';
import { useFocusEffect } from 'expo-router';
import { useClient } from '../core/provider';
import { accountScope, accountScopeKey } from '../core/account-scope.ts';
import type { DrawingReviewPort, DrawingReviewPreview } from '../miniapps/drawing-review-types';
import { NativeButton } from '../components/primitives';
import { t } from '../i18n/index.ts';
import { useTheme } from '../theme';
const unavailable = () => false;
const noChanges = () => () => {};

/** Two bounded native images, exported by the already-admitted editor frame. */
export function DrawingToolReview({
  port,
  requestId,
  contentHash,
  spaceId,
  onReady,
}: {
  port: DrawingReviewPort;
  requestId: string;
  contentHash: string;
  spaceId: string;
  onReady: (ready: boolean) => void;
}) {
  const client = useClient();
  const { store } = client;
  const { colors } = useTheme();
  const ownerNow = accountScopeKey(accountScope(store, client));
  const available = useSyncExternalStore(
    port?.subscribe ?? noChanges,
    port?.getSnapshot ?? unavailable,
    port?.getSnapshot ?? unavailable,
  );
  const [open, setOpen] = useState(false);
  const [attempt, retry] = useState(0);
  const [preview, setPreview] = useState<{
    value: DrawingReviewPreview;
    owner: string;
    key: string;
  } | null>(null);
  const [failure, setFailure] = useState<string | null>(null);
  const lease = useRef({ generation: 0, key: '', loaded: new Set<string>() });
  const visible = preview?.owner === ownerNow && available ? preview : null;
  useFocusEffect(
    useCallback(() => {
      let live = true,
        rendering = false,
        refreshAgain = false;
      const owner = accountScopeKey(accountScope(store));
      const current = () =>
        live &&
        !!owner &&
        accountScopeKey(accountScope(store)) === owner &&
        AppState.currentState === 'active' &&
        port.getSnapshot();
      const retire = () => {
        lease.current.generation++;
        lease.current.key = '';
        lease.current.loaded.clear();
        setPreview(null);
        onReady(false);
      };
      const refresh = async () => {
        retire();
        if (rendering) {
          refreshAgain = true;
          return;
        }
        if (!open) return;
        if (!available || !current()) {
          setFailure(t('miniapps.thisFeatureIsNotAvailableOnThisHost'));
          return;
        }
        const generation = lease.current.generation;
        rendering = true;
        try {
          const value = await port.render(requestId, contentHash);
          if (!current() || lease.current.generation !== generation) return;
          const key = `${owner}:${value.key}:${attempt}:${generation}`;
          lease.current.key = key;
          setPreview({ value, owner, key });
          setFailure(null);
        } catch (reason) {
          if (current() && lease.current.generation === generation)
            setFailure(
              reason instanceof Error && !reason.message.startsWith('drawing_review_')
                ? reason.message
                : t('drawing.unavailable'),
            );
        } finally {
          rendering = false;
          if (refreshAgain && live) {
            refreshAgain = false;
            void refresh();
          }
        }
      };
      void refresh();
      const changed = store.subscribeCanonicalChanges((spaces) => {
        if (!spaces?.length || spaces.includes(spaceId)) void refresh();
      });
      const scope = store.subscribe(() => {
        if (!current()) retire();
      });
      const app = AppState.addEventListener('change', (state) => {
        if (state === 'active') void refresh();
        else retire();
      });
      return () => {
        live = false;
        retire();
        changed();
        scope();
        app.remove();
      };
    }, [store, port, requestId, contentHash, spaceId, onReady, open, available, attempt]),
  );
  const loaded = (side: string, key: string) => {
    if (
      lease.current.key !== key ||
      !visible ||
      visible.key !== key ||
      accountScopeKey(accountScope(store)) !== visible.owner ||
      !port.getSnapshot() ||
      AppState.currentState !== 'active'
    )
      return;
    lease.current.loaded.add(side);
    onReady(lease.current.loaded.has('before') && lease.current.loaded.has('after'));
  };
  return (
    <View style={{ gap: 12 }}>
      {!open || failure ? (
        <NativeButton
          secondary
          testID={`drawing-review-open-${requestId}`}
          label={open ? t('miniapps.tryAgain') : t('agents.reviewRequest')}
          onPress={() => {
            setOpen(true);
            retry((n) => n + 1);
          }}
        />
      ) : null}
      {visible
        ? (['before', 'after'] as const).map((side) => (
            <View key={side} style={{ gap: 6 }}>
              <Text style={{ color: colors.secondary, fontWeight: '600' }}>
                {t(side === 'before' ? 'sites.reviewBefore' : 'sites.reviewAfter')}
              </Text>
              <Image
                testID={`drawing-review-${side}-${requestId}`}
                source={{ uri: visible.value[side].uri }}
                accessibilityLabel={t(
                  side === 'before' ? 'sites.reviewBefore' : 'sites.reviewAfter',
                )}
                cachePolicy="none"
                recyclingKey={`${visible.key}:${side}`}
                contentFit="contain"
                style={{
                  width: '100%',
                  height: 220,
                  borderRadius: 12,
                  backgroundColor: colors.surface,
                }}
                onLoad={() => loaded(side, visible.key)}
                onError={() => {
                  if (lease.current.key !== visible.key) return;
                  lease.current.key = '';
                  lease.current.loaded.clear();
                  setPreview(null);
                  onReady(false);
                  setFailure(t('drawing.unavailable'));
                }}
              />
            </View>
          ))
        : null}
      {failure ? (
        <Text accessibilityRole="alert" style={{ color: colors.danger }}>
          {failure}
        </Text>
      ) : null}
    </View>
  );
}
