import { requireNativeView } from 'expo';
import { Text, type ViewProps } from 'react-native';
import ZoenCore from '../../modules/zoen-core/src/ZoenCoreModule';
import { t } from '../i18n/index.ts';
import { useTheme } from '../theme';
type Props = { html: string; ownerKey: string; active: boolean };
const Preview =
  ZoenCore?.sitePreviewVersion === 1
    ? requireNativeView<Props & ViewProps>('ZoenCore', 'SitePreview')
    : null;
export function SitePreview(props: Props) {
  const { colors } = useTheme();
  return Preview ? (
    <Preview
      {...props}
      accessibilityLabel={t('sites.preview')}
      style={{ flex: 1, backgroundColor: 'white' }}
    />
  ) : (
    <Text style={{ color: colors.secondary, padding: 20 }}>{t('sites.previewUnavailable')}</Text>
  );
}
