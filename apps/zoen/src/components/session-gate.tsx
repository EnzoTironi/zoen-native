import { useState, type ReactNode } from 'react';
import { Image } from 'expo-image';
import { Text, View } from 'react-native';
import { KeyboardAwareScrollView } from 'react-native-keyboard-controller';
import { useClient } from '../core/provider';
import { useTheme } from '../theme';
import { EmptyState, Field, Loading, NativeButton } from './primitives';
import { AccountAccess } from './account-access';

export function SessionGate({ children }: { children: ReactNode }) {
  const { phase, store, notice } = useClient();
  const { colors } = useTheme();
  const [name, setName] = useState('');
  const [handle, setHandle] = useState('');
  const [relay, setRelay] = useState('https://relay.tryzoen.com');
  const [busy, setBusy] = useState(false);
  const [access, setAccess] = useState(false);
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
            onDone={() => setAccess(false)}
            onBack={() => setAccess(false)}
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
                style={{
                  fontSize: 38,
                  fontWeight: '700',
                  letterSpacing: -1.5,
                  color: colors.text,
                }}
              >
                Seu mundo, junto.
              </Text>
              <Text style={{ fontSize: 18, lineHeight: 26, color: colors.secondary }}>
                Pessoas, ideias e agentes. Uma conversa de cada vez.
              </Text>
            </View>
            <Field
              label="Seu nome"
              value={name}
              onChangeText={setName}
              autoComplete="name"
              textContentType="name"
            />
            <Field
              label="Seu @"
              value={handle}
              onChangeText={setHandle}
              autoCapitalize="none"
              autoCorrect={false}
              placeholder="como as pessoas encontram você"
            />
            <Field
              label="Servidor"
              value={relay}
              onChangeText={setRelay}
              autoCapitalize="none"
              autoCorrect={false}
              keyboardType="url"
            />
            {notice ? (
              <Text
                accessibilityRole="alert"
                selectable
                style={{ color: colors.danger, lineHeight: 22 }}
              >
                {notice}
              </Text>
            ) : null}
            <NativeButton
              label={busy ? 'Preparando sua conta…' : 'Criar minha conta'}
              disabled={busy || !name.trim() || !handle.trim()}
              onPress={() => {
                setBusy(true);
                void store
                  .createAccount(name.trim(), handle.trim().replace(/^@/, ''), relay.trim())
                  .catch(() => {})
                  .finally(() => setBusy(false));
              }}
            />
            <NativeButton
              label="Já tenho uma conta"
              secondary
              disabled={busy}
              onPress={() => setAccess(true)}
            />
            {__DEV__ ? (
              <NativeButton
                label="Explorar a demonstração"
                secondary
                onPress={() => {
                  void store.open('demo');
                }}
              />
            ) : null}
            <Text
              style={{
                textAlign: 'center',
                fontSize: 13,
                lineHeight: 20,
                color: colors.secondary,
              }}
            >
              Conversas privadas usam criptografia de ponta a ponta. O nível de privacidade aparece
              nos detalhes de cada conversa. Suas chaves ficam neste aparelho.
            </Text>
          </>
        )}
      </View>
    </KeyboardAwareScrollView>
  );
}
