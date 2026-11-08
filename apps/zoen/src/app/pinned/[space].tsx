import { useLocalSearchParams } from 'expo-router';
import { PinnedList } from '../../pins/pinned-list';
export default function PinnedMessages() {
  const { space, owner } = useLocalSearchParams<{ space: string; owner: string }>();
  return <PinnedList space={space} owner={owner} />;
}
