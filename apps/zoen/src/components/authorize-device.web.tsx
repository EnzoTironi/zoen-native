import { Text } from 'react-native';
import { useTheme } from '../theme';
import { Surface } from './primitives';

export function AuthorizeDevice(_props: { onLinked: () => void }) {
  const { colors } = useTheme();
  return (
    <Surface>
      <Text style={{ color: colors.text, fontSize: 18, fontWeight: '600' }}>
        Conecte novos dispositivos pelo celular
      </Text>
      <Text style={{ color: colors.secondary, lineHeight: 24 }}>
        No Zoen para iOS, abra Você → Dispositivos vinculados. A autorização usa a chave principal
        da conta, guardada no cofre do celular.
      </Text>
    </Surface>
  );
}
