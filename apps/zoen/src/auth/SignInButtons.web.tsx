import { useEffect, useMemo, useRef, useState } from 'react';
import { ActivityIndicator, Text, View } from 'react-native';
import { useTheme } from '../theme';
import { AuthAttempt, type AuthTicket } from './attempt';
import { cancelled } from './guards';
import {
  appleBrowserCredential,
  appleButtonImage,
  browserAuthContext,
  loadAppleIdentity,
  loadGoogleIdentity,
  mountGoogleButton,
} from './browser-providers';
import type { ProviderCredential, SignInButtonsProps } from './types';

export function SignInButtons({
  service,
  configuration,
  scope,
  disabled = false,
  onVerified,
}: SignInButtonsProps) {
  const { colors, scheme } = useTheme();
  const { attempt } = useMemo(
    () => ({ scope, attempt: new AuthAttempt(service) }),
    [service, scope],
  );
  const [revision, setRevision] = useState(0);
  const [width, setWidth] = useState(312);
  const [googleReady, setGoogleReady] = useState(false);
  const [apple, setApple] = useState<{
    sdk: Awaited<ReturnType<typeof loadAppleIdentity>>;
    ticket: AuthTicket;
  } | null>(null);
  const [preparing, setPreparing] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const node = useRef<HTMLDivElement>(null);
  const mounted = useRef(false);
  const running = useRef(false);
  const current = useRef(attempt);
  current.current = attempt;
  const verified = useRef(onVerified);
  verified.current = onVerified;
  const blocked = disabled || busy;
  const disabledRef = useRef(disabled);
  disabledRef.current = disabled;
  const configurationKey = JSON.stringify(configuration);

  useEffect(() => {
    let active = true;
    let removeGoogle: (() => void) | undefined;
    const tickets: AuthTicket[] = [];
    let expiry: ReturnType<typeof setTimeout> | undefined;
    mounted.current = true;
    attempt.activate();
    setPreparing(true);
    setGoogleReady(false);
    setApple(null);
    setBusy(false);
    function failure(reason: unknown) {
      if (active && !cancelled(reason))
        setError(reason instanceof Error ? reason.message : 'Não foi possível preparar a entrada.');
    }
    async function accept(ticket: AuthTicket, credential: ProviderCredential, name?: string) {
      if (!active || disabledRef.current || running.current) return;
      running.current = true;
      setBusy(true);
      setError(null);
      try {
        const result = await attempt.complete(ticket, credential, name);
        if (active && current.current === attempt) await verified.current(result);
      } catch (reason) {
        failure(reason);
      } finally {
        running.current = false;
        if (active) {
          setBusy(false);
          setRevision((value) => value + 1);
        }
      }
    }
    void (async () => {
      try {
        browserAuthContext();
        const work: Promise<void>[] = [];
        if (configuration.google)
          work.push(
            (async () => {
              const [sdk, ticket] = await Promise.all([
                loadGoogleIdentity(),
                attempt.prepare('google', 'web'),
              ]);
              if (!active) {
                attempt.release(ticket);
                return;
              }
              tickets.push(ticket);
              if (node.current)
                removeGoogle = mountGoogleButton(
                  sdk,
                  node.current,
                  ticket,
                  scheme === 'dark',
                  width,
                  (credential) => {
                    void accept(ticket, credential);
                  },
                  failure,
                );
              setGoogleReady(true);
            })().catch(failure),
          );
        if (configuration.apple?.serviceId && configuration.apple.redirectUri)
          work.push(
            (async () => {
              const [sdk, ticket] = await Promise.all([
                loadAppleIdentity(),
                attempt.prepare('apple', 'web'),
              ]);
              if (!active) {
                attempt.release(ticket);
                return;
              }
              tickets.push(ticket);
              setApple({ sdk, ticket });
            })().catch(failure),
          );
        await Promise.all(work);
        if (active && tickets.length)
          expiry = setTimeout(
            () => {
              if (active) setRevision((value) => value + 1);
            },
            Math.max(
              1000,
              Math.min(...tickets.map((ticket) => ticket.challenge.expires_at_ms)) -
                Date.now() -
                10_000,
            ),
          );
      } catch (reason) {
        failure(reason);
      } finally {
        if (active) setPreparing(false);
      }
    })();
    return () => {
      active = false;
      mounted.current = false;
      if (expiry) clearTimeout(expiry);
      removeGoogle?.();
      tickets.forEach((ticket) => attempt.release(ticket));
      attempt.dispose();
    };
    // SDK instances have one owner and public configuration is compared by value.
    // oxlint-disable-next-line react/exhaustive-deps
  }, [attempt, configurationKey, revision, scheme, width]);

  async function continueApple() {
    if (!apple || running.current || blocked || !configuration.apple?.redirectUri) return;
    running.current = true;
    setBusy(true);
    setError(null);
    try {
      const response = await appleBrowserCredential(
        apple.sdk,
        apple.ticket,
        configuration.apple.redirectUri,
      );
      const result = await attempt.complete(
        apple.ticket,
        response.credential,
        response.suggestedName,
      );
      if (mounted.current && current.current === attempt) await onVerified(result);
    } catch (reason) {
      if (!cancelled(reason) && mounted.current && current.current === attempt)
        setError(
          reason instanceof Error ? reason.message : 'Não foi possível continuar. Tente novamente.',
        );
    } finally {
      running.current = false;
      if (mounted.current && current.current === attempt) {
        setBusy(false);
        setRevision((value) => value + 1);
      }
    }
  }
  return (
    <View
      style={{ gap: 12, width: '100%', maxWidth: 375, alignSelf: 'center' }}
      onLayout={(event) =>
        setWidth(Math.max(200, Math.min(375, Math.floor(event.nativeEvent.layout.width))))
      }
    >
      {apple ? (
        <button
          type="button"
          aria-label="Continuar com Apple"
          disabled={blocked}
          onClick={() => {
            void continueApple();
          }}
          style={{
            padding: 0,
            border: 0,
            borderRadius: 12,
            background: 'transparent',
            height: 52,
            width: '100%',
            cursor: blocked ? 'default' : 'pointer',
            opacity: blocked ? 0.5 : 1,
          }}
        >
          <img
            alt="Continuar com Apple"
            src={appleButtonImage(scheme === 'dark', width)}
            style={{ width: '100%', height: 52, display: 'block' }}
          />
        </button>
      ) : !preparing ? (
        <Text style={{ color: colors.secondary }}>Apple indisponível nesta versão.</Text>
      ) : null}
      <div
        ref={node}
        aria-busy={busy}
        inert={blocked}
        style={{
          minHeight: configuration.google ? 44 : 0,
          pointerEvents: blocked ? 'none' : 'auto',
          opacity: blocked ? 0.5 : 1,
        }}
      />
      {!googleReady && !preparing ? (
        <Text style={{ color: colors.secondary }}>Google indisponível nesta versão.</Text>
      ) : null}
      {preparing || busy ? (
        <View
          accessibilityLiveRegion="polite"
          style={{ flexDirection: 'row', alignItems: 'center', gap: 10 }}
        >
          <ActivityIndicator color={colors.accent} />
          <Text style={{ color: colors.secondary }}>
            {busy ? 'Preparando seu Zoen…' : 'Preparando entrada…'}
          </Text>
        </View>
      ) : null}
      {error ? (
        <View style={{ gap: 8 }}>
          <Text
            accessibilityRole="alert"
            selectable
            style={{ color: colors.danger, lineHeight: 22 }}
          >
            {error}
          </Text>
          <button
            type="button"
            onClick={() => {
              setError(null);
              setRevision((value) => value + 1);
            }}
            style={{
              minHeight: 44,
              font: 'inherit',
              color: colors.accent,
              background: 'transparent',
              border: 0,
              textAlign: 'left',
              cursor: 'pointer',
            }}
          >
            Tentar novamente
          </button>
        </View>
      ) : null}
    </View>
  );
}
