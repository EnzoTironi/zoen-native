import { t } from '../i18n/index.ts';
import * as DocumentPicker from 'expo-document-picker';
import { Directory, File, Paths } from 'expo-file-system';
import * as Sharing from 'expo-sharing';
import { markdownFilename, validateMarkdown } from './editing';
import { sensitiveCacheWork } from '../media/cache-work';

export function pickMarkdown(): Promise<{ path: string; markdown: string } | null> {
  return sensitiveCacheWork.run(pickMarkdownCopy);
}

async function pickMarkdownCopy(): Promise<{ path: string; markdown: string } | null> {
  const result = await DocumentPicker.getDocumentAsync({
    type: ['text/markdown', 'text/plain', 'text/x-markdown'],
    copyToCacheDirectory: true,
  });
  if (result.canceled) return null;
  const asset = result.assets[0];
  if (!asset) throw new Error(t('documents.theFileCouldNotBeOpened'));
  const file = new File(asset.uri);
  try {
    if (file.size > 768 * 1024) throw new Error(t('documents.chooseAMarkdownPageUpTo768KB'));
    return { path: asset.name, markdown: validateMarkdown(await file.text()) };
  } finally {
    if (file.exists) file.delete();
  }
}

export function exportMarkdown(title: string, markdown: string): Promise<void> {
  return sensitiveCacheWork.run(() => shareMarkdownCopy(title, markdown));
}

async function shareMarkdownCopy(title: string, markdown: string): Promise<void> {
  if (!(await Sharing.isAvailableAsync()))
    throw new Error(t('documents.thisDeviceDoesNotSupportFileSharing'));
  const directory = new Directory(
    Paths.cache,
    'zoen-exports',
    `${Date.now()}-${Math.random().toString(16).slice(2)}`,
  );
  directory.create({ intermediates: true, idempotent: true });
  const file = new File(directory, markdownFilename(title));
  try {
    file.create();
    await file.write(markdown);
    await Sharing.shareAsync(file.uri, {
      mimeType: 'text/markdown',
      UTI: 'net.daringfireball.markdown',
      dialogTitle: t('documents.exportMarkdownPage'),
    });
  } finally {
    if (directory.exists) directory.delete();
  }
}
