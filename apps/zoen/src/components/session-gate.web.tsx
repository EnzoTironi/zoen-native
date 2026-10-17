import { useEffect, useMemo, useRef, useState, type ReactNode } from 'react';
import { ScrollView, Text, View } from 'react-native';
import { useClient } from '../core/provider';
import type { PairingStatus } from '../core/types';
import { useTheme } from '../theme';
import { SignInButtons } from '../auth/SignInButtons';
import { identityProviders as configuration, relayUrl } from '../runtime-config';
import { createExpoAuthService } from '../auth/service';
import { completeProviderSession, type RecoveryAccount } from '../auth/complete-session';
import { EmptyState, Field, Loading, NativeButton } from './primitives';
import { PairingCode } from './pairing-code';

export function SessionGate({ children }: { children: ReactNode }) {
  const { phase, store, notice, snapshot } = useClient();
  const { colors } = useTheme();
  const [relay, setRelay] = useState(relayUrl);
  const [pairing, setPairing] = useState<PairingStatus | null>(null);
  const [recovering, setRecovering] = useState<RecoveryAccount | null>(null);
  const [advanced, setAdvanced] = useState(false);
  const stop = useRef<(() => void) | null>(null);
  const service = useMemo(() => createExpoAuthService(relay, configuration), [relay]);
  useEffect(() => () => stop.current?.(), []);
  function connect(account?: RecoveryAccount) {
    setRecovering(account ?? null);
    setPairing({ kind: 'starting' });
    stop.current?.();
    try {
      stop.current = store.driver.pair?.(relay, setPairing, account?.identityId) ?? null;
    } catch (error) {
      setPairing({
        kind: 'failed',
        message: error instanceof Error ? error.message : 'Não foi possível preparar a conexão.',
      });
    }
  }
  if (phase === 'ready') return children;
  if (phase === 'opening') return <Loading />;
  if (phase === 'failed')
    return (
      <EmptyState title="Não foi possível abrir o Zoen" detail={notice ?? 'Tente novamente.'}>
        <NativeButton
          label="Tentar novamente"
          onPress={() => {
            void store.open();
          }}
        />
      </EmptyState>
    );
  return (
    <ScrollView
      contentContainerStyle={{
        flexGrow: 1,
        justifyContent: 'center',
        padding: 32,
        backgroundColor: colors.background,
      }}
    >
      <View style={{ maxWidth: 440, width: '100%', alignSelf: 'center', gap: 24 }}>
        <Text style={{ fontSize: 18, fontWeight: '700', color: colors.accent }}>zoen</Text>
        <Text
          accessibilityRole="header"
          style={{
            fontSize: 38,
            fontWeight: '600',
            letterSpacing: -1.5,
            lineHeight: 44,
            color: colors.text,
          }}
        >
          Mais espaço para suas conversas.
        </Text>
        {pairing ? (
          <>
            <Text style={{ color: colors.secondary, lineHeight: 24 }}>
              {recovering
                ? 'Sua conta @' +
                  recovering.handle +
                  ' foi reconhecida. Conecte seu outro aparelho para recuperar as chaves das conversas.'
                : 'Conecte este navegador pelo Zoen no seu outro aparelho.'}
            </Text>
            {pairing.kind === 'starting' ? (
              <Text accessibilityRole="status" style={{ color: colors.secondary }}>
                Preparando sua conexão segura…
              </Text>
            ) : null}
            {pairing.kind === 'waiting' ? (
              <>
                <Text style={{ color: colors.secondary, lineHeight: 24 }}>
                  No celular, abra Você → Dispositivos vinculados → Conectar dispositivo. Confira os
                  seis números antes de aprovar.
                </Text>
                <PairingCode code={pairing.code} check={pairing.check} />
              </>
            ) : null}
            {pairing.kind === 'failed' ? (
              <>
                <Text accessibilityRole="alert" selectable style={{ color: colors.danger }}>
                  {pairing.message}
                </Text>
                <NativeButton
                  label="Tentar conexão novamente"
                  onPress={() => connect(recovering ?? undefined)}
                />
              </>
            ) : null}
            <NativeButton
              label="Voltar"
              secondary
              onPress={() => {
                stop.current?.();
                setPairing(null);
                setRecovering(null);
              }}
            />
          </>
        ) : (
          <>
            <Text style={{ fontSize: 17, lineHeight: 26, color: colors.secondary }}>
              Suas conversas, ideias e agentes, juntos. Entre para começar.
            </Text>
            <SignInButtons
              service={service}
              configuration={configuration}
              scope={relay + ':' + (snapshot?.account?.identity_id ?? 'new')}
              onVerified={(result) =>
                completeProviderSession(
                  result,
                  store.getSnapshot().snapshot?.account?.identity_id,
                  (proof_token, name) =>
                    store.request({ op: 'provider_sign_in', relay_url: relay, proof_token, name }),
                  connect,
                )
              }
            />
            <Text style={{ fontSize: 14, lineHeight: 21, color: colors.secondary }}>
              Seu nome e usuário poderão ser ajustados depois.
            </Text>
            <NativeButton label="Conectar com outro aparelho" secondary onPress={() => connect()} />
          </>
        )}
        {__DEV__ && !pairing ? (
          <>
            <NativeButton
              label={advanced ? 'Fechar opções de desenvolvimento' : 'Opções de desenvolvimento'}
              secondary
              onPress={() => setAdvanced(!advanced)}
            />
            {advanced ? (
              <Field
                label="Servidor"
                value={relay}
                onChangeText={setRelay}
                autoCapitalize="none"
                autoCorrect={false}
              />
            ) : null}
          </>
        ) : null}
        <Text style={{ fontSize: 13, lineHeight: 20, color: colors.secondary }}>
          Conversas privadas usam criptografia de ponta a ponta. Em um novo dispositivo, o histórico
          exige recuperação segura.
        </Text>
      </View>
    </ScrollView>
  );
}
