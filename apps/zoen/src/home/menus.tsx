import { router } from 'expo-router';
import { NativeButton } from '../components/primitives';
import { t } from '../i18n/index.ts';
export function HomeActions() {
  return <NativeButton label={t('home.addMiniapp')} onPress={() => router.push('/miniapps')} />;
}
export function HomeCardMenu({
  name,
  pinned,
  onToggle,
}: {
  name: string;
  pinned: boolean;
  onToggle: () => void;
}) {
  return (
    <NativeButton label={`${t(pinned ? 'home.unpin' : 'home.pin')}: ${name}`} onPress={onToggle} />
  );
}
