import { t } from '../i18n/index.ts';
import type { DeckcraftExportOptions } from './deckcraft-export';
export function shareDeckcraftExport(
  _options: Omit<DeckcraftExportOptions, 'create'>,
): Promise<void> {
  return Promise.reject(new Error(t('documents.thisDeviceDoesNotSupportFileSharing')));
}
