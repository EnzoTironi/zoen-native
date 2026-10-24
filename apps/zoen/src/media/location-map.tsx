import { Text } from 'react-native';
import { t } from '../i18n';
import type { LocationMapProps } from './location-map.types';
export function LocationMap(_props: LocationMapProps) {
  return <Text>{t('attachmentTabs.mapUnavailable')}</Text>;
}
