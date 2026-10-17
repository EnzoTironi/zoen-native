import { Linking, Platform, ScrollView, Text, View } from 'react-native';
import { Stack } from 'expo-router';
import { useNotifications } from '../notifications/provider';
import { useTheme } from '../theme';
import { Icon, NativeButton, Surface } from './primitives';

export function NotificationSettings() {
  const notifications = useNotifications();
  const { colors } = useTheme();
  const status =
    notifications.registration === 'acknowledged'
      ? 'Cadastro confirmado pelo servidor'
      : notifications.registration === 'registering'
        ? 'Confirmando cadastro…'
        : notifications.registration === 'waiting'
          ? 'Aguardando confirmação do servidor'
          : notifications.registration === 'refused'
            ? 'Cadastro recusado pelo servidor'
            : notifications.registration === 'unsupported'
              ? 'Avisos remotos ainda não disponíveis neste aparelho'
              : 'Este aparelho ainda não está cadastrado';
  return (
    <ScrollView
      contentInsetAdjustmentBehavior="automatic"
      contentContainerStyle={{
        padding: 24,
        gap: 24,
        maxWidth: 720,
        width: '100%',
        alignSelf: 'center',
      }}
    >
      <Stack.Screen options={{ title: 'Notificações' }} />
      <Surface>
        <View style={{ flexDirection: 'row', alignItems: 'center', gap: 14 }}>
          <Icon
            name={{ ios: 'bell', android: 'notifications', web: 'notifications' }}
            size={28}
            color={colors.accent}
          />
          <Text
            accessibilityRole="header"
            style={{ color: colors.text, fontSize: 22, fontWeight: '600' }}
          >
            Avisos sob seu controle
          </Text>
        </View>
        <Text style={{ color: colors.secondary, fontSize: 16, lineHeight: 25 }}>
          O servidor envia apenas um sinal para sincronizar. O conteúdo das mensagens é aberto neste
          aparelho. Conversas silenciadas não geram avisos.
        </Text>
      </Surface>
      <Surface>
        <Text
          accessibilityRole="header"
          style={{ color: colors.text, fontSize: 17, fontWeight: '600' }}
        >
          Permissão do aparelho
        </Text>
        <Text style={{ color: colors.secondary, lineHeight: 23 }}>
          {notifications.permission === 'allowed'
            ? 'Permitida'
            : notifications.permission === 'denied'
              ? 'Desativada nos Ajustes'
              : notifications.permission === 'undetermined'
                ? 'Você ainda não escolheu'
                : Platform.OS === 'web'
                  ? 'Indisponível neste navegador'
                  : 'Verificando…'}
        </Text>
        {Platform.OS === 'ios' ? (
          <NativeButton
            label={
              notifications.busy
                ? 'Configurando…'
                : notifications.permission === 'denied'
                  ? 'Abrir Ajustes'
                  : notifications.permission === 'allowed'
                    ? 'Verificar novamente'
                    : 'Permitir notificações'
            }
            disabled={notifications.busy}
            onPress={() => {
              if (notifications.permission === 'denied') void Linking.openSettings();
              else if (notifications.permission === 'allowed') void notifications.refresh();
              else void notifications.requestPermission();
            }}
          />
        ) : (
          <Text style={{ color: colors.secondary, lineHeight: 23 }}>
            As notificações deste navegador ainda não estão disponíveis. Você continua recebendo
            mensagens enquanto o Zoen está aberto.
          </Text>
        )}
      </Surface>
      <Surface>
        <Text
          accessibilityRole="header"
          style={{ color: colors.text, fontSize: 17, fontWeight: '600' }}
        >
          {status}
        </Text>
        <Text style={{ color: colors.secondary, lineHeight: 23 }}>
          O cadastro confirma que o servidor recebeu o pedido. A entrega de avisos neste aparelho
          ainda não foi verificada. As mensagens continuam chegando com o Zoen aberto.
        </Text>
        {notifications.failure ? (
          <Text
            accessibilityRole="alert"
            selectable
            style={{ color: colors.danger, lineHeight: 22 }}
          >
            {notifications.failure}
          </Text>
        ) : null}
      </Surface>
    </ScrollView>
  );
}
