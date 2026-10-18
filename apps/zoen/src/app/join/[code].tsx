import { Redirect, useLocalSearchParams } from 'expo-router';

export default function InvitationLink() {
  const { code } = useLocalSearchParams<{ code: string }>();
  return <Redirect href={{ pathname: '/open-link', params: { kind: 'invite', target: code } }} />;
}
