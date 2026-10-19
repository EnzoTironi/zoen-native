import { useLocalSearchParams } from 'expo-router';
import { RemoteAppScreen } from '../../miniapps/remote-view';

export default function ConnectedApp() {
  const { id } = useLocalSearchParams<{ id: string }>();
  return <RemoteAppScreen id={id} />;
}
