import { useEffect, useRef, useState, type ReactNode } from 'react';
import { ScrollView, Text, View } from 'react-native';
import { useClient } from '../core/provider';
import type { PairingStatus } from '../core/types';
import { useTheme } from '../theme';
import { EmptyState, Field, Loading, NativeButton } from './primitives';
import { PairingCode } from './pairing-code';

export function SessionGate({ children }: { children: ReactNode }) {
  const { phase, store, notice } = useClient();
  const { colors } = useTheme();
  const [relay, setRelay] = useState('https://relay.tryzoen.com');
  const [pairing, setPairing] = useState<PairingStatus | null>(null);
  const stop = useRef<(() => void) | null>(null);
  useEffect(() => () => stop.current?.(), []);
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
        <Text style={{ fontSize: 17, lineHeight: 26, color: colors.secondary }}>
          Conecte este navegador pelo Zoen no seu celular. Conversas privadas usam criptografia de
          ponta a ponta; o nível de privacidade aparece nos detalhes de cada conversa.
        </Text>
        {!pairing || pairing.kind === 'failed' ? (
          <>
            <Field label="Servidor" value={relay} onChangeText={setRelay} autoCapitalize="none" />
            <NativeButton
              label="Conectar este navegador"
              onPress={() => {
                setPairing({ kind: 'starting' });
                stop.current?.();
                stop.current = store.driver.pair?.(relay, setPairing) ?? null;
              }}
            />
          </>
        ) : null}
        {pairing?.kind === 'starting' ? (
          <Text accessibilityRole="status" style={{ color: colors.secondary }}>
            Preparando sua conexão segura…
          </Text>
        ) : null}
        {pairing?.kind === 'waiting' ? (
          <>
            <Text style={{ color: colors.secondary, lineHeight: 24 }}>
              No celular, abra Você → Dispositivos vinculados → Conectar dispositivo. Confira os
              seis números antes de aprovar.
            </Text>
            <PairingCode code={pairing.code} check={pairing.check} />
            <NativeButton
              label="Cancelar conexão"
              secondary
              onPress={() => {
                stop.current?.();
                setPairing(null);
              }}
            />
          </>
        ) : null}
        {pairing?.kind === 'failed' ? (
          <Text accessibilityRole="alert" selectable style={{ color: colors.danger }}>
            {pairing.message}
          </Text>
        ) : null}
      </View>
    </ScrollView>
  );
}
