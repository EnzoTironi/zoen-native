import { useCallback, useMemo, useRef, useState, useSyncExternalStore } from 'react';
import { useFocusEffect } from 'expo-router';
import { CameraView, useCameraPermissions } from 'expo-camera';
import { useClient } from '../core/provider';
import { createEnrollmentPort } from './core-port';
import {
  EnrollmentSession,
  emptyEnrollmentState,
  enrollmentScope,
  enrollmentScopeKey,
  type EnrollmentInputTicket,
} from './session';
import type { ConnectAgentViewProps } from './connect-agent-view';

export function useAgentEnrollment() {
  const { store, mode, snapshot, phase, erasePending } = useClient();
  const scopeKey =
    phase === 'ready' && !erasePending
      ? enrollmentScopeKey(enrollmentScope(mode, snapshot?.account))
      : '';
  const port = useMemo(() => createEnrollmentPort(store), [store]);
  const session = useMemo(() => new EnrollmentSession(port), [port]);
  const state = useSyncExternalStore(session.subscribe, session.getSnapshot, session.getSnapshot);
  const [boot, setBoot] = useState<{
    scope: string;
    availability: ConnectAgentViewProps['availability'];
  }>({ scope: '', availability: 'loading' });
  const [revision, retry] = useState(0);
  const [scannerError, setScannerError] = useState<ConnectAgentViewProps['scannerError']>(null);
  const [permission, requestPermission] = useCameraPermissions();
  const scanning = useRef<EnrollmentInputTicket | null>(null);
  const launching = useRef(false);
  const closeScanner = useCallback(() => {
    const wasScanning = scanning.current !== null;
    scanning.current = null;
    if (wasScanning && process.env.EXPO_OS === 'ios')
      void CameraView.dismissScanner().catch(() => {});
  }, []);

  useFocusEffect(
    useCallback(() => {
      let active = true;
      const current = store.getSnapshot();
      const candidate =
        current.phase === 'ready' && !current.erasePending
          ? enrollmentScope(current.mode, current.snapshot?.account)
          : null;
      const scope = enrollmentScopeKey(candidate) === scopeKey ? candidate : null;
      session.activate(scope);
      setScannerError(null);
      if (!scope || process.env.EXPO_OS !== 'ios') {
        setBoot({ scope: scopeKey, availability: !scope ? 'account-required' : 'unsupported' });
        return () => {
          active = false;
          closeScanner();
          session.retire();
        };
      }
      const ticket = session.inputTicket()!;
      setBoot({ scope: scopeKey, availability: 'loading' });
      void port
        .receipt(scope)
        .then((receipt) => {
          if (!active || !session.accepts(ticket)) return;
          if (receipt) session.restore(receipt, ticket);
          setBoot({ scope: scopeKey, availability: 'ready' });
          if (receipt) void session.reconcile();
        })
        .catch(() => {
          if (active && session.accepts(ticket))
            setBoot({ scope: scopeKey, availability: 'restore-error' });
        });
      const changed = () => {
        if (!active) return;
        const next = store.getSnapshot();
        if (
          next.phase !== 'ready' ||
          next.erasePending ||
          enrollmentScopeKey(enrollmentScope(next.mode, next.snapshot?.account)) !== scopeKey
        ) {
          active = false;
          closeScanner();
          session.retire();
          setBoot({ scope: scopeKey, availability: 'account-required' });
        }
      };
      const unsubscribe = store.subscribe(changed);
      const unregister = store.registerSessionCleanup(async () => {
        active = false;
        closeScanner();
        session.retire();
      });
      const subscription = CameraView.onModernBarcodeScanned((result) => {
        const scan = scanning.current;
        closeScanner();
        if (!active || !scan || !session.accepts(scan)) return;
        setScannerError(null);
        if (session.setCode(result.data, scan)) void session.check();
      });
      return () => {
        active = false;
        subscription.remove();
        unsubscribe();
        unregister();
        closeScanner();
        session.retire();
      };
      // A retry deliberately restarts the read-only probe in the same owner scope.
      // oxlint-disable-next-line react/exhaustive-deps
    }, [store, session, port, scopeKey, revision, closeScanner]),
  );

  const visibleState =
    state.scope === scopeKey ? state : { ...emptyEnrollmentState, scope: scopeKey };
  const availability = boot.scope === scopeKey ? boot.availability : 'loading';
  const scan = async () => {
    if (availability !== 'ready' || launching.current) return;
    const ticket = session.inputTicket();
    if (!ticket) return;
    launching.current = true;
    scanning.current = ticket;
    setScannerError(null);
    try {
      const access = permission?.granted ? permission : await requestPermission();
      if (!session.accepts(ticket)) return;
      if (!access.granted) {
        scanning.current = null;
        setScannerError('denied');
        return;
      }
      await CameraView.launchScanner({ barcodeTypes: ['qr'] });
    } catch {
      if (session.accepts(ticket)) setScannerError('unavailable');
      scanning.current = null;
    } finally {
      launching.current = false;
    }
  };
  return {
    session,
    state: visibleState,
    availability,
    scannerError,
    scannerAvailable: process.env.EXPO_OS === 'ios' && CameraView.isModernBarcodeScannerAvailable,
    scan,
    retryRestore: () => retry((value) => value + 1),
    setCode: (code: string) => {
      if (availability !== 'ready') return;
      setScannerError(null);
      session.setCode(code);
    },
    dismiss: () => {
      closeScanner();
      session.retire();
    },
  };
}
