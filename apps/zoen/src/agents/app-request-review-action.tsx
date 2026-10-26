import { useCallback, useRef, useState } from 'react';
import { router, useFocusEffect } from 'expo-router';
import { Text, View } from 'react-native';
import type { AgentRequestDto } from '../core/generated/AgentRequestDto';
import { accountScope, accountScopeKey } from '../core/account-scope';
import { useClient } from '../core/provider';
import { NativeButton } from '../components/primitives';
import { useTheme } from '../theme';
import { t } from '../i18n';
import { appRequestReviewTarget, resolveAppRequestReview } from '../miniapps/app-request-review';

export function AppRequestReviewAction({ request }: { request: AgentRequestDto }) {
  const client = useClient();
  const { store } = client;
  const { colors } = useTheme();
  const scope = accountScopeKey(accountScope(store, client));
  const target = appRequestReviewTarget(request, scope);
  const [busy, setBusy] = useState(false);
  const [failed, setFailed] = useState(false);
  const key = JSON.stringify([scope, request.id, request.tool_approval?.content_hash]);
  const lifetime = useRef({ active: false, generation: 0, key: '' });
  useFocusEffect(
    useCallback(() => {
      lifetime.current.active = true;
      lifetime.current.key = key;
      setBusy(false);
      setFailed(false);
      return () => {
        lifetime.current.active = false;
        lifetime.current.generation++;
      };
    }, [key, setBusy, setFailed]),
  );
  const tool = request.tool_approval;
  if (!target || !tool) return null;
  return (
    <View style={{ gap: 8 }}>
      <NativeButton
        testID={`app-request-review-${request.id}`}
        label={t('agents.reviewDrawing')}
        disabled={busy}
        onPress={() => {
          if (busy || !lifetime.current.active || lifetime.current.key !== key) return;
          const generation = ++lifetime.current.generation;
          const check = () => {
            if (
              !lifetime.current.active ||
              lifetime.current.generation !== generation ||
              lifetime.current.key !== key
            )
              throw Error('app_review_retired');
          };
          setBusy(true);
          setFailed(false);
          void resolveAppRequestReview(store, target, tool.artifact_id, check).then(
            ({ item }) => {
              if (
                !lifetime.current.active ||
                lifetime.current.generation !== generation ||
                lifetime.current.key !== key ||
                accountScopeKey(accountScope(store)) !== target.scope
              )
                return;
              setBusy(false);
              router.push({
                pathname: '/miniapp/[id]',
                params: {
                  id: item.id,
                  reviewRequest: target.requestId,
                  reviewHash: target.contentHash,
                  reviewScope: target.scope,
                },
              });
            },
            () => {
              if (!lifetime.current.active || lifetime.current.generation !== generation) return;
              setBusy(false);
              setFailed(true);
            },
          );
        }}
      />
      {failed ? (
        <Text accessibilityRole="alert" style={{ color: colors.danger }}>
          {t('agents.requestUnavailable')}
        </Text>
      ) : null}
    </View>
  );
}
