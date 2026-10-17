import { useRef, useState } from 'react';
import { Stack, usePreventRemove } from 'expo-router';
import { Pressable, ScrollView, Text, View } from 'react-native';
import * as Clipboard from 'expo-clipboard';
import { useClient } from '../core/provider';
import { useCoreQuery } from '../core/query';
import { useTheme } from '../theme';
import { useDialogs } from '../components/dialogs';
import { EmptyState, Field, NativeButton, Surface } from '../components/primitives';

export default function Backup() {
  const { store, mode } = useClient();
  const { data, error, retry } = useCoreQuery(mode === 'account' ? { op: 'backup_status' } : null);
  const { colors } = useTheme();
  const { confirm, inform } = useDialogs();
  const [key, setKey] = useState('');
  const [saved, setSaved] = useState(false);
  const [password, setPassword] = useState('');
  const [repeat, setRepeat] = useState('');
  const [method, setMethod] = useState<'recovery' | 'password'>('recovery');
  const [busy, setBusy] = useState(false);
  const [failure, setFailure] = useState<string | null>(null);
  const [status, setStatus] = useState<string | null>(null);
  const [leaving, setLeaving] = useState(false);
  const confirming = useRef(false);
  usePreventRemove(!leaving && (!!key || !!password || busy), ({ repeat: leave }) => {
    if (busy) {
      void inform('Backup em andamento', 'Espere esta operação terminar antes de sair.');
      return;
    }
    if (confirming.current) return;
    confirming.current = true;
    void confirm({
      title: 'Sair da configuração do backup?',
      message: key
        ? 'A chave de recuperação deixará de aparecer nesta tela. Guarde-a em um lugar seguro antes de sair.'
        : 'A senha preenchida será descartada.',
      confirmLabel: 'Sair da configuração',
      cancelLabel: 'Continuar aqui',
    })
      .then((accepted) => {
        if (accepted) {
          setLeaving(true);
          leave();
        }
      })
      .finally(() => {
        confirming.current = false;
      });
  });
  async function perform(action: () => Promise<void>) {
    if (busy) return;
    setBusy(true);
    setFailure(null);
    setStatus(null);
    try {
      await action();
    } catch (reason) {
      setFailure(reason instanceof Error ? reason.message : 'Não foi possível concluir o backup.');
    } finally {
      retry();
      setBusy(false);
    }
  }
  if (mode === 'demo')
    return (
      <EmptyState
        title="Backup da sua conta"
        detail="Entre na sua conta para proteger seu histórico com um backup criptografado."
      />
    );
  return (
    <ScrollView
      contentInsetAdjustmentBehavior="automatic"
      keyboardShouldPersistTaps="handled"
      contentContainerStyle={{ padding: 24, gap: 20 }}
    >
      <Stack.Screen options={{ title: 'Backup criptografado' }} />
      <Text style={{ color: colors.secondary, lineHeight: 24, fontSize: 16 }}>
        Seu backup protege o histórico e as chaves da conta. Guarde a chave de recuperação ou a
        senha em um lugar seguro. O Zoen não consegue recuperá-las por você.
      </Text>
      <Surface>
        <Text style={{ color: colors.text, fontSize: 20, fontWeight: '600' }}>
          {data
            ? data.enabled
              ? 'Backup ativado'
              : 'Backup desativado'
            : 'Conferindo seu backup…'}
        </Text>
        {data?.enabled ? (
          <>
            <Text style={{ color: colors.secondary }}>
              Protegido por {data.mode === 'recovery_key' ? 'chave de recuperação' : 'senha'}.
            </Text>
            <Text style={{ color: colors.secondary }}>
              {data.last_backup_ms !== '0'
                ? `Último envio: ${new Date(Number(data.last_backup_ms)).toLocaleString('pt-BR')}`
                : 'O primeiro envio ainda não terminou.'}
            </Text>
            <NativeButton
              label={busy ? 'Aguarde…' : 'Fazer backup agora'}
              disabled={busy}
              onPress={() => {
                void perform(async () => {
                  await store.driver.request({ op: 'backup_now' });
                  setStatus('Backup enviado e protegido.');
                });
              }}
            />
            <NativeButton
              secondary
              label="Desativar e apagar backup"
              disabled={busy}
              onPress={() => {
                void perform(async () => {
                  if (
                    !(await confirm({
                      title: 'Apagar o backup do servidor?',
                      message:
                        'Você perderá esta opção de recuperação até ativar um novo backup. Os dados nos seus aparelhos permanecem disponíveis.',
                      confirmLabel: 'Apagar backup',
                      destructive: true,
                    }))
                  )
                    return;
                  await store.driver.request({ op: 'backup_turn_off' });
                  setStatus('Backup apagado do servidor.');
                });
              }}
            />
          </>
        ) : null}
      </Surface>
      {data && (!data.enabled || key) ? (
        <Surface>
          <Text style={{ color: colors.text, fontSize: 19, fontWeight: '600' }}>
            Como você quer recuperar sua conta?
          </Text>
          <View style={{ flexDirection: 'row', gap: 10, flexWrap: 'wrap' }}>
            <NativeButton
              label="Chave de recuperação"
              secondary={method !== 'recovery'}
              disabled={busy || !!key}
              onPress={() => setMethod('recovery')}
            />
            <NativeButton
              label="Senha do backup"
              secondary={method !== 'password'}
              disabled={busy || !!key}
              onPress={() => setMethod('password')}
            />
          </View>
          {method === 'recovery' ? (
            key ? (
              <>
                <Text style={{ color: colors.secondary, lineHeight: 24 }}>
                  Guarde estes 64 caracteres antes de ativar. Quem tiver esta chave poderá abrir seu
                  backup.
                </Text>
                <Text
                  selectable
                  style={{
                    color: colors.text,
                    fontFamily: 'monospace',
                    fontSize: 17,
                    lineHeight: 28,
                  }}
                >
                  {key.match(/.{1,4}/g)?.join(' ')}
                </Text>
                <NativeButton
                  secondary
                  label="Copiar chave de recuperação"
                  onPress={() => {
                    void Clipboard.setStringAsync(key, { android: { isSensitive: true } })
                      .then((copied) => {
                        if (!copied) throw new Error('Não foi possível copiar.');
                        setStatus('Chave copiada. Guarde-a no seu gerenciador de senhas.');
                      })
                      .catch(() => setFailure('Não foi possível copiar a chave.'));
                  }}
                />
                <Pressable
                  accessibilityRole="checkbox"
                  accessibilityState={{ checked: saved }}
                  disabled={busy}
                  onPress={() => setSaved((value) => !value)}
                  style={{
                    paddingVertical: 14,
                    flexDirection: 'row',
                    gap: 12,
                    alignItems: 'center',
                  }}
                >
                  <Text style={{ color: colors.accent, fontSize: 24 }}>{saved ? '☑' : '☐'}</Text>
                  <Text style={{ color: colors.text, flex: 1, lineHeight: 23 }}>
                    Guardei esta chave em um lugar seguro fora do Zoen.
                  </Text>
                </Pressable>
                <NativeButton
                  label={busy ? 'Protegendo seu backup…' : 'Ativar meu backup'}
                  disabled={busy || !saved}
                  onPress={() => {
                    void perform(async () => {
                      await store.driver.request({
                        op: 'backup_enable_recovery_key',
                        recovery_key: key,
                      });
                      setKey('');
                      setSaved(false);
                      setStatus('Backup ativado e enviado.');
                    });
                  }}
                />
              </>
            ) : (
              <NativeButton
                label="Gerar minha chave"
                disabled={busy}
                onPress={() => {
                  void perform(async () => {
                    setKey(await store.driver.request({ op: 'backup_create_recovery_key' }));
                    setSaved(false);
                  });
                }}
              />
            )
          ) : (
            <>
              <Field
                label="Nova senha do backup"
                value={password}
                onChangeText={setPassword}
                secureTextEntry
                autoComplete="new-password"
                textContentType="newPassword"
                autoCapitalize="none"
                autoCorrect={false}
                editable={!busy}
              />
              <Field
                label="Repita a senha"
                value={repeat}
                onChangeText={setRepeat}
                secureTextEntry
                autoComplete="new-password"
                textContentType="newPassword"
                autoCapitalize="none"
                autoCorrect={false}
                editable={!busy}
              />
              <Text style={{ color: colors.secondary, lineHeight: 22 }}>
                Use pelo menos 12 caracteres e guarde a senha no seu gerenciador de senhas.
              </Text>
              <NativeButton
                label={busy ? 'Protegendo seu backup…' : 'Ativar com esta senha'}
                disabled={busy || password.length < 12 || password !== repeat}
                onPress={() => {
                  void perform(async () => {
                    await store.driver.request({ op: 'backup_password', password });
                    setPassword('');
                    setRepeat('');
                    setStatus('Backup ativado e enviado.');
                  });
                }}
              />
            </>
          )}
        </Surface>
      ) : null}
      {status ? (
        <Text accessibilityRole="status" style={{ color: colors.secondary, lineHeight: 23 }}>
          {status}
        </Text>
      ) : null}
      {failure || error ? (
        <Text selectable accessibilityRole="alert" style={{ color: colors.danger, lineHeight: 23 }}>
          {failure ?? error}
        </Text>
      ) : null}
      {error ? (
        <NativeButton secondary label="Tentar novamente" disabled={busy} onPress={retry} />
      ) : null}
    </ScrollView>
  );
}
