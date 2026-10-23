import { Text } from 'react-native';
import { t } from '../i18n/index.ts';
import { useTheme } from '../theme';
export type SitePreviewProps = { html: string; ownerKey: string; active: boolean };
export function SitePreview(_props: SitePreviewProps) {
  const { colors } = useTheme();
  return (
    <Text style={{ color: colors.secondary, padding: 20 }}>{t('sites.previewUnavailable')}</Text>
  );
}
