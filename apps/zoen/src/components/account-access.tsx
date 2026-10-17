import { useState } from 'react';
import { Text, View } from 'react-native';
import { useClient } from '../core/provider';
import type { LinkRequestDto } from '../core/generated/LinkRequestDto';
import { useTheme } from '../theme';
import { Field, NativeButton, Surface } from './primitives';
import { PairingCode } from './pairing-code';

const message = (error: unknown) =>
  error instanceof Error ? error.message : 'Não foi possível concluir. Tente novamente.';

export function AccountAccess({
  relay,
  expectedIdentityId,
  expectedHandle,
  onDone,
  onBack,
}: {
  relay: string;
  expectedIdentityId?: string;
  expectedHandle?: string;
  onDone: () => void;
  onBack: () => void;
}) {
  const { store } = useClient();
  const { colors } = useTheme();
  const [method, setMethod] = useState<'link' | 'recovery' | 'password'>('link');
  const [request, setRequest] = useState<LinkRequestDto | null>(null);
  const [handle, setHandle] = useState(expectedHandle ?? '');
  const [secret, setSecret] = useState('');
  const [busy, setBusy] = useState(false);
  const [linked, setLinked] = useState(false);
  const [status, setStatus] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  async function run(action: () => Promise<void>) {
    if (busy) return;
    setBusy(true);
    setError(null);
    try {
      await action();
    } catch (reason) {
      setError(message(reason));
    } finally {
      setBusy(false);
    }
  }
  async function history() {
    const current = await store.driver.request({ op: 'history_status' });
    if (current.receive_active) {
      setStatus(
        'Seu histórico está sendo recebido. Você pode entrar no Zoen e acompanhar em Dispositivos.',
      );
      return;
    }
    if (!current.initial_pending) {
      setStatus('Seu histórico já foi recebido. Você pode entrar no Zoen.');
      return;
    }
    setStatus('Recebendo seu histórico…');
    const result = await store.driver.request({ op: 'receive_history', timeout_ms: 15_000 });
    setStatus(`${result.messages} mensagens recebidas. Seu histórico está pronto.`);
    await store.refresh();
  }
  return (
    <View style={{ gap: 20 }}>
      <Text
        accessibilityRole="header"
        style={{ color: colors.text, fontSize: 28, fontWeight: '600' }}
      >
        Sua conta, neste aparelho.
      </Text>
      {expectedHandle ? (
        <Text style={{ color: colors.secondary, lineHeight: 24 }}>
          Sua conta @{expectedHandle} foi reconhecida. Recupere as chaves pelo outro aparelho ou
          pelo backup para abrir suas conversas.
        </Text>
      ) : null}
      {!linked ? (
        <View style={{ flexDirection: 'row', flexWrap: 'wrap', gap: 8 }}>
          {(
            [
              { value: 'link', label: 'Outro aparelho' },
              { value: 'recovery', label: 'Chave de recuperação' },
              { value: 'password', label: 'Senha do backup' },
            ] as const
          ).map((choice) => (
            <NativeButton
              key={choice.value}
              secondary={method !== choice.value}
              disabled={busy}
              label={choice.label}
              onPress={() => {
                setMethod(choice.value);
                setSecret('');
                setError(null);
              }}
            />
          ))}
        </View>
      ) : null}
      {linked ? (
        <Surface>
          <Text style={{ color: colors.text, fontSize: 18, fontWeight: '600' }}>
            Conta conectada
          </Text>
          <Text style={{ color: colors.secondary, lineHeight: 24 }}>
            Mantenha o outro aparelho conectado para receber o histórico. Você também pode continuar
            agora: a transferência continua automaticamente e pode ser acompanhada em Dispositivos.
          </Text>
          <NativeButton
            label="Receber histórico"
            disabled={busy}
            onPress={() => {
              void run(history);
            }}
          />
          <NativeButton
            secondary
            label="Entrar no Zoen"
            disabled={busy}
            onPress={() => {
              void run(async () => {
                await store.refresh();
                onDone();
              });
            }}
          />
        </Surface>
      ) : method === 'link' ? (
        <>
          <Text style={{ color: colors.secondary, lineHeight: 24 }}>
            No aparelho que já usa sua conta, abra Você → Dispositivos vinculados → Conectar
            dispositivo. Escaneie o QR ou cole o código e compare os seis números antes de aprovar.
          </Text>
          {request ? <PairingCode code={request.code} check={request.check} /> : null}
          <NativeButton
            label={
              busy
                ? 'Aguardando aprovação…'
                : request
                  ? 'Já aprovei no outro aparelho'
                  : 'Gerar código de conexão'
            }
            disabled={busy || !relay.trim()}
            onPress={() => {
              void run(async () => {
                if (!request) {
                  setRequest(
                    await store.driver.request({ op: 'link_request', relay_url: relay.trim() }),
                  );
                  return;
                }
                await store.driver.request({
                  op: 'link_wait',
                  timeout_ms: 10_000,
                  expected_identity_id: expectedIdentityId ?? null,
                });
                setLinked(true);
                setStatus('Conta conectada. Envie o histórico pelo outro aparelho.');
                await store.refresh();
              });
            }}
          />
        </>
      ) : (
        <>
          <Text style={{ color: colors.secondary, lineHeight: 24 }}>
            Restaure o backup criptografado que você ativou anteriormente. A{' '}
            {method === 'recovery' ? 'chave' : 'senha'} do backup é necessária para abrir seus
            dados.
          </Text>
          <Field
            label="Seu @"
            value={handle}
            onChangeText={setHandle}
            editable={!busy && !expectedHandle}
            autoCapitalize="none"
            autoCorrect={false}
          />
          <Field
            label={method === 'recovery' ? 'Chave de recuperação' : 'Senha do backup'}
            value={secret}
            onChangeText={setSecret}
            editable={!busy}
            secureTextEntry
            autoCapitalize="none"
            autoCorrect={false}
            autoComplete="off"
          />
          <NativeButton
            label={busy ? 'Restaurando seu backup…' : 'Restaurar minha conta'}
            disabled={busy || !handle.trim() || !secret.trim() || !relay.trim()}
            onPress={() => {
              void run(async () => {
                const common = {
                  relay_url: relay.trim(),
                  handle: handle.trim().replace(/^@/, ''),
                  expected_identity_id: expectedIdentityId ?? null,
                };
                await store.driver.request(
                  method === 'recovery'
                    ? {
                        op: 'restore_backup_recovery_key',
                        ...common,
                        recovery_key: secret.replace(/\s/g, ''),
                      }
                    : { op: 'restore_backup_password', ...common, password: secret },
                );
                setSecret('');
                await store.refresh();
                onDone();
              });
            }}
          />
        </>
      )}
      {status ? (
        <Text accessibilityRole="status" style={{ color: colors.secondary, lineHeight: 23 }}>
          {status}
        </Text>
      ) : null}
      {error ? (
        <Text selectable accessibilityRole="alert" style={{ color: colors.danger, lineHeight: 23 }}>
          {error}
        </Text>
      ) : null}
      {!linked ? <NativeButton secondary label="Voltar" disabled={busy} onPress={onBack} /> : null}
    </View>
  );
}
