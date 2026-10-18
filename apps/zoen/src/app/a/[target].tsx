import { Redirect, useLocalSearchParams } from 'expo-router';

export default function AgentLink() {
  const { target } = useLocalSearchParams<{ target: string }>();
  return <Redirect href={{ pathname: '/open-link', params: { kind: 'agent', target } }} />;
}
