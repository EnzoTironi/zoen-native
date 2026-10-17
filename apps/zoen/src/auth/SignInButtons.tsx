import { useEffect, useMemo, useRef, useState } from 'react';
import { ActivityIndicator, Text, View } from 'react-native';
import { useTheme } from '../theme';
import { AuthAttempt, type AuthTicket } from './attempt';
import { cancelled } from './guards';
import { loadNativeProviders, nativeCredential, type NativeProviders } from './native-providers';
import type { AuthProvider, SignInButtonsProps } from './types';

/** Native, approved provider buttons. Parent automatically provisions or recovers keys. */
export function SignInButtons({
  service,
  configuration,
  scope,
  disabled = false,
  onVerified,
}: SignInButtonsProps) {
  const { scheme, colors } = useTheme();
  const { attempt } = useMemo(
    () => ({ scope, attempt: new AuthAttempt(service) }),
    [service, scope],
  );
  const key = JSON.stringify(configuration);
  const [modules, setModules] = useState<NativeProviders | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const mounted = useRef(false);
  const latest = useRef(attempt);
  latest.current = attempt;
  const running = useRef(false);
  useEffect(() => {
    let current = true;
    mounted.current = true;
    attempt.activate();
    setModules(null);
    setError(null);
    setBusy(false);
    void loadNativeProviders(configuration)
      .then((value) => {
        if (current) setModules(value);
      })
      .catch(() => {
        if (current) setModules({});
      });
    return () => {
      current = false;
      mounted.current = false;
      attempt.dispose();
    };
    // Configuration contains public scalar identifiers and is compared by value.
    // oxlint-disable-next-line react/exhaustive-deps
  }, [attempt, key]);
  async function signIn(provider: AuthProvider) {
    if (!modules || running.current || disabled || busy) return;
    running.current = true;
    setBusy(true);
    setError(null);
    let ticket: AuthTicket | undefined;
    try {
      ticket = await attempt.prepare(provider, 'ios');
      const response = await nativeCredential(ticket, modules, configuration);
      const result = await attempt.complete(ticket, response.credential, response.suggestedName);
      if (mounted.current && latest.current === attempt) await onVerified(result);
    } catch (reason) {
      if (!cancelled(reason) && mounted.current && latest.current === attempt)
        setError(
          reason instanceof Error ? reason.message : 'Não foi possível continuar. Tente novamente.',
        );
    } finally {
      if (ticket) attempt.release(ticket);
      running.current = false;
      if (mounted.current && latest.current === attempt) setBusy(false);
    }
  }
  const Apple = modules?.apple;
  const Google = modules?.google?.GoogleSignInButton;
  const blocked = disabled || busy;
  return (
    <View style={{ gap: 12, width: '100%' }}>
      {Apple ? (
        <View pointerEvents={blocked ? 'none' : 'auto'}>
          <Apple.AppleAuthenticationButton
            buttonType={Apple.AppleAuthenticationButtonType.CONTINUE}
            buttonStyle={
              scheme === 'dark'
                ? Apple.AppleAuthenticationButtonStyle.WHITE
                : Apple.AppleAuthenticationButtonStyle.BLACK
            }
            cornerRadius={12}
            style={{ height: 52, width: '100%' }}
            accessibilityState={{ disabled: blocked, busy }}
            onPress={() => {
              void signIn('apple');
            }}
          />
        </View>
      ) : modules ? (
        <Text style={{ color: colors.secondary }}>Apple indisponível nesta versão.</Text>
      ) : null}
      {Google ? (
        <Google
          signInBehavior="none"
          colorScheme={scheme}
          size="wide"
          style={{ width: '100%', height: 52 }}
          disabled={blocked}
          loading={busy}
          onPress={() => signIn('google')}
          accessibilityLabel="Continuar com Google"
        />
      ) : modules ? (
        <Text style={{ color: colors.secondary }}>Google indisponível nesta versão.</Text>
      ) : null}
      {modules === null || busy ? (
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
        <Text accessibilityRole="alert" selectable style={{ color: colors.danger, lineHeight: 22 }}>
          {error}
        </Text>
      ) : null}
    </View>
  );
}
