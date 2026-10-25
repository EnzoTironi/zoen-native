import { useCallback, useEffect, useLayoutEffect, useRef, useState } from 'react';
import { router, useFocusEffect } from 'expo-router';
import { AppState, Text, View } from 'react-native';
import { useClient } from '../core/provider';
import type { Command, ItemDetail } from '../core/types';
import { documentScope } from '../documents/document-authority';
import { homeSpacesFor } from '../home/access';
import { NativeButton } from '../components/primitives';
import { t } from '../i18n/index';
import { useTheme } from '../theme';
import { AppPackageIntent, isAppPackage, type AppPackageSource } from './package-intent';
import { AppPackageRelease } from './package-release';

export function AppPackageAction({
  item,
  target,
  onUpdated,
}: {
  item: ItemDetail;
  target?: ItemDetail;
  onUpdated?: () => void;
}) {
  const { store } = useClient();
  const scope = documentScope(store.getSnapshot(), store.sessionEpoch);
  const account = store.getSnapshot().snapshot?.account;
  if (!isAppPackage(item) || !item.file || !scope || !account) return null;
  if (target && (!target.app || target.space_id !== item.space_id)) return null;
  return (
    <PackageAction
      key={JSON.stringify([scope, item.id, item.version, item.file.sha256, target?.id])}
      scope={scope}
      fileTitle={item.title}
      target={target}
      onUpdated={onUpdated}
      source={{
        expected_identity_id: account.identity_id,
        expected_device_id: account.device_id,
        space_id: item.space_id,
        package_item_id: item.id,
        package_version: item.version,
        package_sha256: item.file.sha256,
        title: target?.title ?? item.title,
      }}
    />
  );
}

function PackageAction({
  scope,
  fileTitle,
  source: input,
  target,
  onUpdated,
}: {
  scope: string;
  fileTitle: string;
  source: AppPackageSource;
  target?: ItemDetail;
  onUpdated?: () => void;
}) {
  const { store } = useClient();
  const { colors } = useTheme();
  const [source] = useState(() => Object.freeze({ ...input }));
  const [targetPin] = useState(() =>
    target?.app ? { itemId: target.id, resourceUri: target.app.resource_uri } : null,
  );
  const intent = useRef<AppPackageIntent | AppPackageRelease | null>(null);
  const flight = useRef<object | null>(null);
  const callbacks = useRef(onUpdated);
  useLayoutEffect(() => {
    callbacks.current = onUpdated;
  }, [onUpdated]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [activation, setActivation] = useState<object>({});
  const foreground = useRef<object | null>(activation);
  useEffect(() => {
    const subscription = AppState.addEventListener('change', (state) => {
      intent.current?.close();
      intent.current = null;
      const next = state === 'active' ? {} : null;
      foreground.current = next;
      if (next) setActivation(next);
    });
    return () => subscription.remove();
  }, []);
  const current = useCallback(
    () =>
      AppState.currentState === 'active' &&
      foreground.current === activation &&
      !store.isSessionRetiring &&
      documentScope(store.getSnapshot(), store.sessionEpoch) === scope &&
      homeSpacesFor(store.getSnapshot()).has(source.space_id),
    [store, scope, source.space_id, activation],
  );
  useFocusEffect(
    useCallback(() => {
      if (!current()) return;
      const request = <C extends Command>(command: C) => store.request(command);
      const action = targetPin
        ? new AppPackageRelease(source, targetPin.itemId, targetPin.resourceUri, request, current)
        : new AppPackageIntent(source, request, current);
      intent.current = action;
      flight.current = null;
      setBusy(false);
      setError(null);
      const unregister = store.registerSessionCleanup(async () => action.close());
      const unsubscribe = store.subscribe(() => {
        if (!current()) action.close();
      });
      return () => {
        action.close();
        unregister();
        unsubscribe();
        if (intent.current === action) intent.current = null;
      };
    }, [store, source, current, targetPin]),
  );
  return (
    <View style={{ gap: 8 }}>
      <NativeButton
        testID={targetPin ? 'app-package-update' : 'app-package-open'}
        label={targetPin ? fileTitle : t('miniapps.openAppPackage')}
        disabled={busy}
        onPress={() => {
          const action = intent.current;
          if (!action || flight.current || !current()) return;
          const operation = {};
          flight.current = operation;
          setBusy(true);
          setError(null);
          const task = Promise.resolve().then(() =>
            action instanceof AppPackageRelease ? action.update() : action.open(),
          );
          void task
            .then((item) => {
              if (intent.current !== action || !current()) return;
              if ('item_id' in item) callbacks.current?.();
              else router.push({ pathname: '/miniapp/[id]', params: { id: item.id } });
            })
            .catch((reason: unknown) => {
              if (intent.current === action && current())
                setError(
                  reason instanceof Error
                    ? reason.message
                    : t('miniapps.theMiniappCouldNotBeOpened'),
                );
            })
            .finally(() => {
              if (flight.current !== operation) return;
              flight.current = null;
              if (intent.current === action && current()) setBusy(false);
            });
        }}
      />
      {error ? (
        <Text accessibilityRole="alert" style={{ color: colors.danger }}>
          {error}
        </Text>
      ) : null}
    </View>
  );
}
