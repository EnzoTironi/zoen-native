import { PinReview } from '../../../pins/pin-review';
import { useLocalSearchParams } from 'expo-router';
export default function MessagePin() {
  const { space, message, owner } = useLocalSearchParams<{
    space: string;
    message: string;
    owner: string;
  }>();
  return <PinReview space={space} message={message} owner={owner} />;
}
