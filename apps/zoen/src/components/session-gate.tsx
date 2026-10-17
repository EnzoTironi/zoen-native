import { useMemo, useState, type ReactNode } from 'react';
import { Image } from 'expo-image';
import { Text, View } from 'react-native';
import { KeyboardAwareScrollView } from 'react-native-keyboard-controller';
import { useClient } from '../core/provider';
import { useTheme } from '../theme';
import { SignInButtons } from '../auth/SignInButtons';
import { identityProviders as configuration, relayUrl } from '../runtime-config';
import { createExpoAuthService } from '../auth/service';
import { completeProviderSession, type RecoveryAccount } from '../auth/complete-session';
import { EmptyState, Field, Loading, NativeButton } from './primitives';
import { AccountAccess } from './account-access';

export function SessionGate({ children }: { children: ReactNode }) {
  const { phase } = useClient();
  if (phase === 'erasing') return <Loading label="Saindo deste aparelho…" />;
  return <SessionContent>{children}</SessionContent>;
}

function SessionContent({ children }: { children: ReactNode }) {
  const { phase, store, notice, snapshot } = useClient();
  const { colors } = useTheme();
  const [relay, setRelay] = useState(relayUrl);
  const [access, setAccess] = useState(false);
  const [advanced, setAdvanced] = useState(false);
  const [recovering, setRecovering] = useState<RecoveryAccount | null>(null);
  const service = useMemo(() => createExpoAuthService(relay, configuration), [relay]);
  if (phase === 'ready' && !access) return children;
  if (phase === 'opening') return <Loading />;
  if (phase === 'failed')
    return (
      <View style={{ flex: 1, backgroundColor: colors.background }}>
        <EmptyState
          title="Vamos tentar de novo"
          detail={notice ?? 'Não foi possível abrir o Zoen.'}
        >
          <NativeButton
            label="Tentar novamente"
            onPress={() => {
              void store.open();
            }}
          />
        </EmptyState>
      </View>
    );
  return (
    <KeyboardAwareScrollView
      style={{ flex: 1, backgroundColor: colors.background }}
      bottomOffset={24}
      contentInsetAdjustmentBehavior="automatic"
      keyboardShouldPersistTaps="handled"
      keyboardDismissMode="interactive"
      contentContainerStyle={{ flexGrow: 1, justifyContent: 'center', padding: 28 }}
    >
      <View style={{ width: '100%', maxWidth: 420, alignSelf: 'center', gap: 24 }}>
        {access ? (
          <AccountAccess
            relay={relay}
            expectedIdentityId={recovering?.identityId}
            expectedHandle={recovering?.handle}
            onDone={() => setAccess(false)}
            onBack={() => {
              setAccess(false);
              setRecovering(null);
            }}
          />
        ) : (
          <>
            <Image
              source={require('../../assets/images/icon.png')}
              style={{ width: 84, height: 84, borderRadius: 26 }}
              accessibilityLabel="Zoen"
            />
            <View style={{ gap: 10 }}>
              <Text
                accessibilityRole="header"
                style={{ fontSize: 38, fontWeight: '700', letterSpacing: -1.5, color: colors.text }}
              >
                Seu mundo, junto.
              </Text>
              <Text style={{ fontSize: 18, lineHeight: 26, color: colors.secondary }}>
                Pessoas, ideias e agentes. Uma conversa de cada vez.
              </Text>
            </View>
            {notice && snapshot?.account?.setup_pending ? (
              <Text accessibilityRole="alert" style={{ color: colors.danger, lineHeight: 22 }}>
                {notice}
              </Text>
            ) : null}
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
                  (account) => {
                    setRecovering(account);
                    setAccess(true);
                  },
                )
              }
            />
            <Text style={{ fontSize: 14, lineHeight: 21, color: colors.secondary }}>
              Seu nome e usuário poderão ser ajustados depois.
            </Text>
            <NativeButton label="Recuperar acesso" secondary onPress={() => setAccess(true)} />
            {__DEV__ ? (
              <>
                <NativeButton
                  label="Explorar a demonstração"
                  secondary
                  onPress={() => {
                    void store.open('demo');
                  }}
                />
                <NativeButton
                  label={
                    advanced ? 'Fechar opções de desenvolvimento' : 'Opções de desenvolvimento'
                  }
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
                    keyboardType="url"
                  />
                ) : null}
              </>
            ) : null}
            <Text
              style={{ textAlign: 'center', fontSize: 13, lineHeight: 20, color: colors.secondary }}
            >
              Conversas privadas usam criptografia de ponta a ponta. Permissões e backup serão
              apresentados quando você precisar.
            </Text>
          </>
        )}
      </View>
    </KeyboardAwareScrollView>
  );
}
