import { Stack } from 'expo-router';
import { EmptyState } from '../components/primitives';

export default function Backup() {
  return (
    <>
      <Stack.Screen options={{ title: 'Backup criptografado' }} />
      <EmptyState
        title="Proteja sua conta pelo celular"
        detail="No Zoen para iOS, abra Você → Backup criptografado. Este navegador guarda apenas a chave deste dispositivo; a chave principal da conta permanece no cofre do celular."
      />
    </>
  );
}
