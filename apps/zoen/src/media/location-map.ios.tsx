import { requireNativeView, requireOptionalNativeModule } from 'expo';
import { Text } from 'react-native';
import { t } from '../i18n';
import type { LocationPointEvent } from '../../modules/zoen-core/src/ZoenLocation.types';
import { nativeLocationPoint } from './native-location';
import type { LocationMapProps } from './location-map.types';
import type { ViewProps } from 'react-native';
const bridge = requireOptionalNativeModule<{ staticLocationVersion?: number }>('ZoenCore');
const NativeMap =
  bridge?.staticLocationVersion === 1
    ? requireNativeView<
        ViewProps & {
          configuration: string;
          onPointChange(event: LocationPointEvent): void;
        }
      >('ZoenCore', 'ZoenLocationMap')
    : null;

export function LocationMap({ binding, editable, point, label, onPointChange }: LocationMapProps) {
  if (!NativeMap) return <Text>{t('attachmentTabs.mapUnavailable')}</Text>;
  return (
    <NativeMap
      style={{ height: 260, width: '100%' }}
      accessibilityLabel={label}
      configuration={JSON.stringify({
        binding,
        editable,
        point,
        accessibilityLabel: label,
        selectPointLabel: t('attachmentTabs.selectMapCenter'),
      })}
      onPointChange={(event) => {
        if (!editable || event.nativeEvent.binding !== binding) return;
        try {
          onPointChange?.(nativeLocationPoint(event.nativeEvent.point));
        } catch {
          /* Invalid/retired native events have no draft authority. */
        }
      }}
    />
  );
}
