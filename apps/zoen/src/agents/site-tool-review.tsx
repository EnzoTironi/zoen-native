import { useCallback, useState } from 'react';
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
  const [review, setReview] = useState<Review | null>(null);
  const [before, setBefore] = useState(false);
  const [failure, setFailure] = useState<string | null>(null);
  const native = Platform.OS === 'ios' && NativeCore?.sitePreviewVersion === 1;
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
        setReview(null);
        onReady(false);
      };
      const refresh = async () => {
        retire();
        if (!current() || !scope) return;
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
          setBefore(false);
          setReview(next);
          setFailure(null);
          onReady(native);
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
    }, [store, requestId, contentHash, spaceId, native, onReady]),
  );
  return (
    <View style={{ gap: 8 }}>
      {review ? (
        <>
          {review.before_html !== null ? (
            <View style={{ flexDirection: 'row', gap: 8 }}>
              <NativeButton
                secondary={before}
                label={t('sites.reviewAfter')}
                onPress={() => setBefore(false)}
              />
              <NativeButton
                secondary={!before}
                label={t('sites.reviewBefore')}
                onPress={() => setBefore(true)}
              />
            </View>
          ) : null}
          <View style={{ height: 280, overflow: 'hidden', borderRadius: 12 }}>
            <SitePreview
              html={before ? (review.before_html ?? review.after_html) : review.after_html}
              ownerKey={`${requestId}:${contentHash}:${before ? 'before' : 'after'}`}
              active
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
