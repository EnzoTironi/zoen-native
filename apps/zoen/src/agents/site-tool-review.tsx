import { useCallback, useRef, useState } from 'react';
import { AppState, Platform, Text, View } from 'react-native';
import { useFocusEffect } from 'expo-router';
import NativeCore from '../../modules/zoen-core/src/ZoenCoreModule';
import { useClient } from '../core/provider';
import type { SiteToolReview as Review } from '../core/generated/SiteToolReview';
import { siteAuthority, siteScope } from '../sites/source';
import { SitePreview } from '../sites/preview';
import { NativeButton } from '../components/primitives';
import { t } from '../i18n/index.ts';
import { useTheme } from '../theme';

/** The reviewed bytes come from the signed request, never an editable DOM or a
 * model-supplied URL. Losing focus/authority destroys the disposable preview. */
export function SiteToolReview({
  requestId,
  contentHash,
  spaceId,
  onReady,
}: {
  requestId: string;
  contentHash: string;
  spaceId: string;
  onReady: (ready: boolean) => void;
}) {
  const { store } = useClient();
  const { colors } = useTheme();
  const lease = useRef<{ sequence: number; key: string | null }>({ sequence: 0, key: null });
  const [review, setReview] = useState<{
    value: Review;
    owner: string;
    key: string;
    before: boolean;
  } | null>(null);
  const scopeNow = siteScope(store, spaceId);
  const ownerNow = scopeNow && siteAuthority(store, scopeNow);
  const visible = review && review.owner === ownerNow ? review : null;
  const [failure, setFailure] = useState<string | null>(null);
  const native = Platform.OS === 'ios' && (NativeCore?.sitePreviewVersion ?? 0) >= 2;
  useFocusEffect(
    useCallback(() => {
      let live = true;
      let generation = 0;
      const scope = siteScope(store, spaceId);
      const owner = scope && siteAuthority(store, scope);
      const current = () =>
        live &&
        AppState.currentState === 'active' &&
        !!scope &&
        !!owner &&
        siteAuthority(store, scope) === owner;
      const retire = () => {
        generation++;
        lease.current.key = null;
        setReview(null);
        onReady(false);
      };
      const refresh = async () => {
        retire();
        if (!current() || !scope || !owner) return;
        const operation = generation;
        try {
          const next = await store.driver.request({
            op: 'site_tool_review',
            expected_identity_id: scope.identity_id,
            expected_device_id: scope.device_id,
            request_id: requestId,
            expected_hash: contentHash,
          });
          if (!current() || generation !== operation) return;
          if (next.request_id !== requestId || next.content_hash !== contentHash)
            throw new Error(t('sites.changedConsent'));
          const key = `${owner}:${requestId}:${contentHash}:${++lease.current.sequence}`;
          lease.current.key = key;
          setReview({ value: next, owner, key, before: false });
          setFailure(null);
        } catch (reason) {
          if (current() && generation === operation)
            setFailure(reason instanceof Error ? reason.message : t('sites.unavailable'));
        }
      };
      void refresh();
      const canonical = store.subscribeCanonicalChanges(() => {
        void refresh();
      });
      const ownerChange = store.subscribe(() => {
        if (!current()) retire();
      });
      const preflight = store.registerSessionPreflight(retire);
      const app = AppState.addEventListener('change', (state) => {
        if (state === 'active') void refresh();
        else retire();
      });
      return () => {
        live = false;
        retire();
        canonical();
        ownerChange();
        preflight();
        app.remove();
      };
    }, [store, requestId, contentHash, spaceId, onReady]),
  );
  const show = (before: boolean) => {
    if (!visible || lease.current.key !== visible.key) return;
    const key = `${visible.owner}:${requestId}:${contentHash}:${++lease.current.sequence}`;
    lease.current.key = key;
    onReady(false);
    setReview({ ...visible, key, before });
  };
  return (
    <View style={{ gap: 8 }}>
      {visible ? (
        <>
          {visible.value.before_html !== null ? (
            <View style={{ flexDirection: 'row', gap: 8 }}>
              <NativeButton
                secondary={visible.before}
                label={t('sites.reviewAfter')}
                onPress={() => show(false)}
              />
              <NativeButton
                secondary={!visible.before}
                label={t('sites.reviewBefore')}
                onPress={() => show(true)}
              />
            </View>
          ) : null}
          <View style={{ height: 280, overflow: 'hidden', borderRadius: 12 }}>
            <SitePreview
              html={
                visible.before
                  ? (visible.value.before_html ?? visible.value.after_html)
                  : visible.value.after_html
              }
              ownerKey={visible.key}
              active
              onPreviewStatus={({ nativeEvent }) => {
                const scope = siteScope(store, spaceId);
                if (
                  lease.current.key !== visible.key ||
                  nativeEvent.ownerKey !== visible.key ||
                  !scope ||
                  siteAuthority(store, scope) !== visible.owner ||
                  AppState.currentState !== 'active'
                )
                  return;
                onReady(native && nativeEvent.ready && !visible.before);
              }}
            />
          </View>
        </>
      ) : null}
      {failure ? (
        <Text accessibilityRole="alert" style={{ color: colors.danger }}>
          {failure}
        </Text>
      ) : null}
    </View>
  );
}
