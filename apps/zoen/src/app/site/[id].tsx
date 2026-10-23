import { useLocalSearchParams } from 'expo-router';
import { SiteDocument } from '../../sites/document';
export default function Site() {
  const { id, version } = useLocalSearchParams<{ id: string; version?: string }>();
  const selected = version && /^[1-9]\d*$/u.test(version) ? Number(version) : undefined;
  return <SiteDocument key={`${id}:${version ?? ''}`} itemId={id} version={selected} />;
}
